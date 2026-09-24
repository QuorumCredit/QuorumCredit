//! Standardized pagination cursor encoding, decoding, and validation.
//!
//! Cursors are opaque, URL-safe tokens that encode the position of the last
//! item returned by a paginated endpoint. The canonical format is a
//! base64url-encoded (no padding) JSON payload with the following shape:
//!
//! ```json
//! { "v": 1, "k": "<sort key>", "t": "<tiebreaker>", "e": 1700000000 }
//! ```
//!
//! * `v` - cursor format version (currently [`CURSOR_VERSION`]).
//! * `k` - the primary sort key of the last item on the page.
//! * `t` - a stable tiebreaker (e.g. record id) to disambiguate equal keys.
//! * `e` - optional expiry as a Unix timestamp in seconds.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

/// Current cursor format version. Bump when the payload shape changes.
pub const CURSOR_VERSION: u8 = 1;

/// Errors produced while decoding or validating a pagination cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorError {
    /// The cursor was empty or not valid base64url.
    Malformed,
    /// The decoded payload was not valid JSON or had an unexpected shape.
    InvalidPayload,
    /// The cursor was produced by an unsupported format version.
    UnsupportedVersion(u8),
    /// The cursor has expired.
    Expired,
}

impl CursorError {
    /// Stable, machine-readable error code for API responses.
    pub fn code(&self) -> &'static str {
        match self {
            CursorError::Malformed => "invalid_cursor",
            CursorError::InvalidPayload => "invalid_cursor",
            CursorError::UnsupportedVersion(_) => "unsupported_cursor_version",
            CursorError::Expired => "expired_cursor",
        }
    }

    /// Human-readable message suitable for an API error body.
    pub fn message(&self) -> String {
        match self {
            CursorError::Malformed => "The pagination cursor is malformed.".to_string(),
            CursorError::InvalidPayload => {
                "The pagination cursor payload is invalid.".to_string()
            }
            CursorError::UnsupportedVersion(v) => {
                format!("Unsupported pagination cursor version: {v}.")
            }
            CursorError::Expired => "The pagination cursor has expired.".to_string(),
        }
    }
}

impl std::fmt::Display for CursorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message())
    }
}

impl std::error::Error for CursorError {}

/// The canonical, versioned cursor payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    /// Format version.
    pub v: u8,
    /// Primary sort key of the last item on the page.
    pub k: String,
    /// Stable tiebreaker (e.g. record id) for equal sort keys.
    pub t: String,
    /// Optional expiry as a Unix timestamp in seconds.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub e: Option<u64>,
}

impl Cursor {
    /// Build a cursor for the given sort key and tiebreaker.
    pub fn new(key: impl Into<String>, tiebreaker: impl Into<String>) -> Self {
        Cursor {
            v: CURSOR_VERSION,
            k: key.into(),
            t: tiebreaker.into(),
            e: None,
        }
    }

    /// Attach an expiry `ttl_secs` seconds from now.
    pub fn with_ttl(mut self, ttl_secs: u64) -> Self {
        self.e = Some(now_unix() + ttl_secs);
        self
    }

    /// Encode the cursor into its opaque, URL-safe token form.
    pub fn encode(&self) -> String {
        let json = serde_json::to_vec(self).expect("cursor serialization is infallible");
        URL_SAFE_NO_PAD.encode(json)
    }

    /// Decode and validate an opaque cursor token.
    pub fn decode(token: &str) -> Result<Cursor, CursorError> {
        if token.is_empty() {
            return Err(CursorError::Malformed);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(token.as_bytes())
            .map_err(|_| CursorError::Malformed)?;
        let cursor: Cursor =
            serde_json::from_slice(&bytes).map_err(|_| CursorError::InvalidPayload)?;
        cursor.validate()?;
        Ok(cursor)
    }

    /// Validate version and expiry of a decoded cursor.
    pub fn validate(&self) -> Result<(), CursorError> {
        if self.v != CURSOR_VERSION {
            return Err(CursorError::UnsupportedVersion(self.v));
        }
        if self.k.is_empty() || self.t.is_empty() {
            return Err(CursorError::InvalidPayload);
        }
        if let Some(exp) = self.e {
            if exp <= now_unix() {
                return Err(CursorError::Expired);
            }
        }
        Ok(())
    }
}

/// Encode a cursor from a sort key and tiebreaker.
pub fn encode_cursor(key: impl Into<String>, tiebreaker: impl Into<String>) -> String {
    Cursor::new(key, tiebreaker).encode()
}

/// Decode and validate an opaque cursor token.
pub fn decode_cursor(token: &str) -> Result<Cursor, CursorError> {
    Cursor::decode(token)
}

/// Validate an optional cursor token, returning the decoded cursor when present.
///
/// This is the entry point used by paginated endpoints and by the cursor
/// validation middleware: a `None` cursor is valid (first page), while a
/// malformed or expired cursor yields a [`CursorError`].
pub fn validate_cursor(token: Option<&str>) -> Result<Option<Cursor>, CursorError> {
    match token {
        None => Ok(None),
        Some(t) => Cursor::decode(t).map(Some),
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_cursor() {
        let token = encode_cursor("2024-01-01T00:00:00Z", "42");
        let decoded = decode_cursor(&token).expect("valid cursor");
        assert_eq!(decoded.v, CURSOR_VERSION);
        assert_eq!(decoded.k, "2024-01-01T00:00:00Z");
        assert_eq!(decoded.t, "42");
    }

    #[test]
    fn rejects_malformed_cursor() {
        assert_eq!(decode_cursor("not-a-cursor!!"), Err(CursorError::Malformed));
    }

    #[test]
    fn rejects_expired_cursor() {
        let mut cursor = Cursor::new("k", "t");
        cursor.e = Some(1);
        assert_eq!(cursor.validate(), Err(CursorError::Expired));
    }

    #[test]
    fn none_cursor_is_valid() {
        assert_eq!(validate_cursor(None), Ok(None));
    }
}
