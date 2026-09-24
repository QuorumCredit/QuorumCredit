//! Standardized pagination cursor support for API endpoints.
//!
//! Cursors are encoded as base64url (no padding) of a compact JSON payload:
//! `{"v":1,"k":<sort key>,"t":<tiebreaker>}`. This gives every paginated
//! endpoint a single, versioned, self-describing cursor format.

use std::fmt;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};

/// Current cursor format version. Bump when the payload shape changes.
pub const CURSOR_VERSION: u8 = 1;

/// Canonical cursor payload shared by all paginated endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor {
    /// Format version, used to reject cursors from incompatible releases.
    pub v: u8,
    /// Primary sort key value of the last item on the previous page.
    pub k: String,
    /// Tiebreaker value (e.g. id) to keep ordering stable across equal keys.
    pub t: String,
}

impl Cursor {
    /// Build a cursor for the current format version.
    pub fn new(sort_key: impl Into<String>, tiebreaker: impl Into<String>) -> Self {
        Self {
            v: CURSOR_VERSION,
            k: sort_key.into(),
            t: tiebreaker.into(),
        }
    }

    /// Encode this cursor into the standardized opaque string form.
    pub fn encode(&self) -> String {
        let json = serde_json::to_vec(self).expect("cursor payload is always serializable");
        URL_SAFE_NO_PAD.encode(json)
    }

    /// Decode a standardized cursor string, validating version and shape.
    pub fn decode(raw: &str) -> Result<Self, CursorError> {
        if raw.is_empty() {
            return Err(CursorError::Empty);
        }

        let bytes = URL_SAFE_NO_PAD
            .decode(raw)
            .map_err(|_| CursorError::Malformed)?;

        let cursor: Cursor = serde_json::from_slice(&bytes).map_err(|_| CursorError::Malformed)?;

        if cursor.v != CURSOR_VERSION {
            return Err(CursorError::UnsupportedVersion(cursor.v));
        }

        if cursor.k.is_empty() || cursor.t.is_empty() {
            return Err(CursorError::Malformed);
        }

        Ok(cursor)
    }
}

/// Errors produced while decoding a standardized cursor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CursorError {
    /// No cursor value was supplied.
    Empty,
    /// The cursor was not valid base64url or did not match the payload shape.
    Malformed,
    /// The cursor was produced by an incompatible format version.
    UnsupportedVersion(u8),
}

impl fmt::Display for CursorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CursorError::Empty => write!(f, "cursor must not be empty"),
            CursorError::Malformed => write!(f, "cursor is malformed"),
            CursorError::UnsupportedVersion(v) => {
                write!(f, "unsupported cursor version: {v}")
            }
        }
    }
}

impl std::error::Error for CursorError {}

/// Consistent error body returned when a cursor fails validation.
#[derive(Debug, Clone, Serialize)]
pub struct CursorErrorResponse {
    pub error: &'static str,
    pub message: String,
}

impl From<CursorError> for CursorErrorResponse {
    fn from(err: CursorError) -> Self {
        Self {
            error: "invalid_cursor",
            message: err.to_string(),
        }
    }
}

/// Validate an optional cursor query parameter.
///
/// `None` means the client requested the first page and is always valid.
/// `Some(raw)` is decoded with the standardized format, returning a
/// consistent error response when the cursor is malformed or unsupported.
pub fn validate_cursor(raw: Option<&str>) -> Result<Option<Cursor>, CursorErrorResponse> {
    match raw {
        None => Ok(None),
        Some(value) => Cursor::decode(value).map(Some).map_err(Into::into),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_standard_cursor() {
        let cursor = Cursor::new("2024-01-01T00:00:00Z", "42");
        let encoded = cursor.encode();
        assert_eq!(Cursor::decode(&encoded).unwrap(), cursor);
    }

    #[test]
    fn rejects_malformed_cursor() {
        assert_eq!(Cursor::decode("not-a-cursor"), Err(CursorError::Malformed));
    }

    #[test]
    fn rejects_empty_cursor() {
        assert_eq!(Cursor::decode(""), Err(CursorError::Empty));
    }

    #[test]
    fn rejects_unsupported_version() {
        let payload = serde_json::json!({"v": 99, "k": "a", "t": "b"});
        let encoded = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap());
        assert_eq!(
            Cursor::decode(&encoded),
            Err(CursorError::UnsupportedVersion(99))
        );
    }

    #[test]
    fn validate_cursor_allows_first_page() {
        assert!(validate_cursor(None).unwrap().is_none());
    }

    #[test]
    fn validate_cursor_maps_errors_to_response() {
        let err = validate_cursor(Some("bad")).unwrap_err();
        assert_eq!(err.error, "invalid_cursor");
    }
}
