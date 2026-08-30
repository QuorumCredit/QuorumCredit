//! Cross-Chain Auction Module
//!
//! Implements auctions for liquidating defaulted loans across multiple chains.
//! Allows collateral (slashed stakes) to be auctioned off to recover value
//! and distribute proceeds to remaining vouchers.
//!
//! Issue #974: Auction defaulted loans across chains
//!
//! Key features:
//! - English auction for slashed collateral
//! - Cross-chain bid aggregation
//! - Settlement and collateral transfer across chains
//! - Fallback liquidation mechanisms

use crate::errors::ContractError;
use crate::helpers::require_admin_approval;
use crate::types::DataKey;
use soroban_sdk::{contracttype, Address, Env, Vec};

/// Auction for a defaulted loan's slashed collateral
#[contracttype]
#[derive(Clone)]
pub struct CrossChainAuction {
    /// Unique auction ID
    pub auction_id: u64,
    /// Borrower whose loan defaulted
    pub defaulted_borrower: Address,
    /// Total collateral available for auction (slashed stakes)
    pub collateral_amount: i128,
    /// Token being auctioned
    pub token: Address,
    /// When auction starts (ledger seconds)
    pub auction_start: u64,
    /// When auction ends
    pub auction_end: u64,
    /// Minimum acceptable bid
    pub reserve_price: i128,
    /// Current highest bid
    pub highest_bid: i128,
    /// Address of highest bidder
    pub highest_bidder: Option<Address>,
    /// Total bids received
    pub bid_count: u32,
    /// Whether settlement has occurred
    pub settled: bool,
    /// Chains where auction is active
    pub participating_chains: Vec<u32>,
}

/// Individual bid on an auction
#[contracttype]
#[derive(Clone)]
pub struct Bid {
    pub auction_id: u64,
    pub bidder: Address,
    pub amount: i128,
    pub timestamp: u64,
    pub chain_id: u32,
    /// Whether bid was already refunded/returned
    pub refunded: bool,
}

/// Auction settlement record
#[contracttype]
#[derive(Clone)]
pub struct AuctionSettlement {
    pub auction_id: u64,
    pub winning_bid: i128,
    pub winning_bidder: Address,
    pub total_proceeds: i128,
    pub settled_at: u64,
    /// Amount distributed to vouchers
    pub voucher_payout: i128,
    /// Amount sent to protocol treasury
    pub treasury_payout: i128,
}

/// Auction status for queries
#[contracttype]
#[derive(Clone, Copy)]
pub enum AuctionStatus {
    /// Auction not yet started
    Pending,
    /// Auction currently accepting bids
    Active,
    /// Auction ended, awaiting settlement
    Ended,
    /// Auction settled and collateral transferred
    Settled,
}

/// Admin: Create a new cross-chain auction for defaulted loan collateral
pub fn create_cross_chain_auction(
    env: Env,
    admin_signers: Vec<Address>,
    defaulted_borrower: Address,
    collateral_amount: i128,
    token: Address,
    auction_duration_seconds: u64,
    reserve_price: i128,
    participating_chains: Vec<u32>,
) -> Result<u64, ContractError> {
    require_admin_approval(&env, &admin_signers);

    if collateral_amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    if auction_duration_seconds == 0 {
        return Err(ContractError::InvalidAmount);
    }

    if reserve_price < 0 {
        return Err(ContractError::InvalidAmount);
    }

    // Generate auction ID (timestamp-based)
    let auction_id = env.ledger().timestamp() as u64;
    let now = env.ledger().timestamp();

    let auction = CrossChainAuction {
        auction_id,
        defaulted_borrower,
        collateral_amount,
        token,
        auction_start: now,
        auction_end: now + auction_duration_seconds,
        reserve_price,
        highest_bid: 0,
        highest_bidder: None,
        bid_count: 0,
        settled: false,
        participating_chains,
    };

    env.storage()
        .persistent()
        .set(&DataKey::CrossChainAuction(auction_id), &auction);

    Ok(auction_id)
}

/// Place a bid on an active auction
pub fn place_bid(
    env: Env,
    auction_id: u64,
    bidder: Address,
    bid_amount: i128,
    chain_id: u32,
) -> Result<(), ContractError> {
    bidder.require_auth();

    if bid_amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let mut auction: CrossChainAuction = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainAuction(auction_id))
        .ok_or(ContractError::NoActiveLoan)?;

    let now = env.ledger().timestamp();

    // Check auction is active
    if now < auction.auction_start || now >= auction.auction_end {
        return Err(ContractError::InvalidStateTransition);
    }

    // Check bid meets reserve and beats highest bid
    if bid_amount < auction.reserve_price {
        return Err(ContractError::InsufficientFunds);
    }

    if bid_amount <= auction.highest_bid {
        return Err(ContractError::InsufficientFunds);
    }

    // Refund previous highest bid
    if let Some(prev_bidder) = auction.highest_bidder.clone() {
        let prev_bid = Bid {
            auction_id,
            bidder: prev_bidder.clone(),
            amount: auction.highest_bid,
            timestamp: now,
            chain_id,
            refunded: true,
        };
        // In production, would transfer funds back to prev_bidder
        env.storage()
            .persistent()
            .set(&DataKey::AuctionBid(auction_id, prev_bidder), &prev_bid);
    }

    // Record new highest bid
    auction.highest_bid = bid_amount;
    auction.highest_bidder = Some(bidder.clone());
    auction.bid_count += 1;

    let bid = Bid {
        auction_id,
        bidder: bidder.clone(),
        amount: bid_amount,
        timestamp: now,
        chain_id,
        refunded: false,
    };

    env.storage()
        .persistent()
        .set(&DataKey::AuctionBid(auction_id, bidder), &bid);

    env.storage()
        .persistent()
        .set(&DataKey::CrossChainAuction(auction_id), &auction);

    Ok(())
}

/// End auction and settle with highest bidder.
///
/// Callable by any address, not just an admin, once `now >= auction_end` — the
/// `auction_end` check below is the only gate. Settlement should not depend on
/// an admin remembering to show up: while it does, slashed collateral stays
/// locked and vouchers never receive their auction proceeds. See
/// `docs/cross-chain-trust-model.md` for the resulting keeper-incentive gap.
pub fn settle_auction(env: Env, auction_id: u64) -> Result<(), ContractError> {
    let mut auction: CrossChainAuction = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainAuction(auction_id))
        .ok_or(ContractError::NoActiveLoan)?;

    let now = env.ledger().timestamp();

    // Check auction has ended
    if now < auction.auction_end {
        return Err(ContractError::InvalidStateTransition);
    }

    // Check not already settled
    if auction.settled {
        return Err(ContractError::InvalidStateTransition);
    }

    // If no bids or reserve not met, mark as failed
    if auction.highest_bidder.is_none() || auction.highest_bid < auction.reserve_price {
        auction.settled = true;
        env.storage()
            .persistent()
            .set(&DataKey::CrossChainAuction(auction_id), &auction);
        return Err(ContractError::InsufficientFunds);
    }

    let winning_bidder = auction.highest_bidder.clone().unwrap();
    let winning_bid = auction.highest_bid;

    // Calculate settlement splits (80% to vouchers, 20% to treasury)
    let voucher_payout = (winning_bid * 80) / 100;
    let treasury_payout = winning_bid - voucher_payout;

    let settlement = AuctionSettlement {
        auction_id,
        winning_bid,
        winning_bidder: winning_bidder.clone(),
        total_proceeds: winning_bid,
        settled_at: now,
        voucher_payout,
        treasury_payout,
    };

    // Mark auction as settled
    auction.settled = true;

    env.storage()
        .persistent()
        .set(&DataKey::CrossChainAuction(auction_id), &auction);
    env.storage()
        .persistent()
        .set(&DataKey::AuctionSettlement(auction_id), &settlement);

    // In production, would:
    // 1. Transfer collateral to winning bidder
    // 2. Distribute proceeds to vouchers and treasury
    // 3. Emit settlement events across participating chains

    Ok(())
}

/// Get auction status
pub fn get_auction_status(env: Env, auction_id: u64) -> Result<AuctionStatus, ContractError> {
    let auction: CrossChainAuction = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainAuction(auction_id))
        .ok_or(ContractError::NoActiveLoan)?;

    let now = env.ledger().timestamp();

    if auction.settled {
        Ok(AuctionStatus::Settled)
    } else if now >= auction.auction_end {
        Ok(AuctionStatus::Ended)
    } else if now >= auction.auction_start {
        Ok(AuctionStatus::Active)
    } else {
        Ok(AuctionStatus::Pending)
    }
}

/// Get current auction details
pub fn get_auction(env: Env, auction_id: u64) -> Result<CrossChainAuction, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::CrossChainAuction(auction_id))
        .ok_or(ContractError::NoActiveLoan)
}

/// Get settlement details if auction is settled
pub fn get_auction_settlement(
    env: Env,
    auction_id: u64,
) -> Result<AuctionSettlement, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::AuctionSettlement(auction_id))
        .ok_or(ContractError::NoActiveLoan)
}

/// Query bid history for an auction
pub fn get_auction_bid(
    env: Env,
    auction_id: u64,
    bidder: Address,
) -> Result<Bid, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::AuctionBid(auction_id, bidder))
        .ok_or(ContractError::NoActiveLoan)
}

/// Extend an in-progress auction to a new (later) end time. Admin/authorized
/// signers only.
pub fn extend_auction(
    env: Env,
    admin_signers: Vec<Address>,
    auction_id: u64,
    new_auction_end: u64,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    let mut auction: CrossChainAuction = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainAuction(auction_id))
        .ok_or(ContractError::NoActiveLoan)?;

    // Can only extend if no bids yet or before auction end
    if auction.bid_count > 0 && env.ledger().timestamp() >= auction.auction_end {
        return Err(ContractError::InvalidStateTransition);
    }

    // An "extension" must move the end time later, never earlier or in place.
    if new_auction_end <= auction.auction_end {
        return Err(ContractError::InvalidStateTransition);
    }

    auction.auction_end = new_auction_end;

    env.storage()
        .persistent()
        .set(&DataKey::CrossChainAuction(auction_id), &auction);

    Ok(())
}

/// Cancel auction if no significant bids (admin only)
pub fn cancel_auction(
    env: Env,
    admin_signers: Vec<Address>,
    auction_id: u64,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    let mut auction: CrossChainAuction = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainAuction(auction_id))
        .ok_or(ContractError::NoActiveLoan)?;

    if auction.settled {
        return Err(ContractError::InvalidStateTransition);
    }

    // Unwind the current highest bid, if any, so cancelling mid-flight doesn't
    // leave that bidder's stake looking like it's still in play.
    if let Some(highest_bidder) = auction.highest_bidder.clone() {
        let bid_key = DataKey::AuctionBid(auction_id, highest_bidder);
        if let Some(mut bid) = env.storage().persistent().get::<DataKey, Bid>(&bid_key) {
            bid.refunded = true;
            env.storage().persistent().set(&bid_key, &bid);
        }
    }

    // Mark as settled with zero proceeds (failed auction)
    auction.settled = true;

    env.storage()
        .persistent()
        .set(&DataKey::CrossChainAuction(auction_id), &auction);

    // In production, would also refund any earlier outbid bidders' funds.

    Ok(())
}
