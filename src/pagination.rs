use crate::types::{PaginationParams, PaginatedLoans, PaginatedVouches, LoanRecord, VouchRecord};
use soroban_sdk::{Vec, Env};

/// Default pagination limit
const DEFAULT_LIMIT: u32 = 10;
/// Maximum pagination limit
const MAX_LIMIT: u32 = 100;

/// Validate and normalize pagination parameters
pub fn normalize_pagination(limit: Option<u32>, offset: Option<u32>) -> PaginationParams {
    let limit = limit.unwrap_or(DEFAULT_LIMIT);
    let limit = if limit > MAX_LIMIT { MAX_LIMIT } else { limit };
    let offset = offset.unwrap_or(0);
    
    PaginationParams { limit, offset }
}

/// Paginate a vector of loan records
pub fn paginate_loans(
    loans: Vec<LoanRecord>,
    total: u32,
    limit: u32,
    offset: u32,
) -> PaginatedLoans {
    let start = offset as usize;
    let end = (offset + limit) as usize;
    let end = if end > loans.len() { loans.len() } else { end };
    
    let mut paginated = Vec::new(&soroban_sdk::Env::new());
    if start < loans.len() {
        for i in start..end {
            if let Some(loan) = loans.get(i as u32) {
                paginated.push_back(loan);
            }
        }
    }
    
    PaginatedLoans {
        loans: paginated,
        total,
        limit,
        offset,
    }
}

/// Paginate a vector of vouch records
pub fn paginate_vouches(
    vouches: Vec<VouchRecord>,
    total: u32,
    limit: u32,
    offset: u32,
) -> PaginatedVouches {
    let start = offset as usize;
    let end = (offset + limit) as usize;
    let end = if end > vouches.len() { vouches.len() } else { end };
    
    let mut paginated = Vec::new(&soroban_sdk::Env::new());
    if start < vouches.len() {
        for i in start..end {
            if let Some(vouch) = vouches.get(i as u32) {
                paginated.push_back(vouch);
            }
        }
    }
    
    PaginatedVouches {
        vouches: paginated,
        total,
        limit,
        offset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_pagination_defaults() {
        let params = normalize_pagination(None, None);
        assert_eq!(params.limit, DEFAULT_LIMIT);
        assert_eq!(params.offset, 0);
    }

    #[test]
    fn test_normalize_pagination_max_limit() {
        let params = normalize_pagination(Some(200), None);
        assert_eq!(params.limit, MAX_LIMIT);
    }
}
