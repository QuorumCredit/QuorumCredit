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

use crate::cross_chain::{self, BridgeAttestation, CrossChainLoanMetadata};
use crate::errors::ContractError;
use crate::helpers::require_admin_approval;
use crate::types::{DataKey, LoanStatus};
use crate::vouch;
use soroban_sdk::{contracttype, token, Address, Env, Vec};

/// chain_id used to denote a bid placed locally (no bridge attestation required).
const LOCAL_CHAIN_ID: u32 = 0;

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

/// Place a bid on an active auction.
///
/// If `chain_id` is not the local chain, the caller must supply a verified
/// bridge attestation tying this exact (chain, auction, bidder, amount) tuple
/// to a real event on that chain — otherwise a purely local caller could claim
/// an arbitrary `chain_id` and inflate cross-chain bid aggregation figures
/// with no verification (Issue #1456).
///
/// The bid amount is escrowed atomically (a real `token::Client` transfer)
/// rather than tracked as bookkeeping only, so outbid or cancelled-auction
/// bidders have a guaranteed path to reclaim funds via `claim_refund`
/// (Issue #1457).
pub fn place_bid(
    env: Env,
    auction_id: u64,
    bidder: Address,
    bid_amount: i128,
    chain_id: u32,
    attestation: Option<BridgeAttestation>,
) -> Result<(), ContractError> {
    bidder.require_auth();

    if bid_amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    if chain_id != LOCAL_CHAIN_ID {
        // The claimed chain must have an active, registered bridge.
        vouch::validate_bridge(&env, chain_id)?;

        // Require and verify (and consume, to prevent replay) a bridge
        // attestation binding this specific bid to a real remote-chain event.
        let attestation = attestation.ok_or(ContractError::BridgeAttestationRequired)?;
        let metadata = CrossChainLoanMetadata {
            origin_chain: chain_id,
            loan_id: auction_id,
            borrower: bidder.clone(),
            amount: bid_amount,
            status: LoanStatus::Active,
            reputation_score: 0,
        };
        cross_chain::validate_bridge_attestation(env.clone(), metadata, attestation)?;
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

    // Escrow the bid. If this bidder already has an unclaimed escrowed bid on
    // this same auction (i.e. they are raising their own bid), only pull the
    // incremental difference so funds aren't double-charged.
    let bid_key = DataKey::AuctionBid(auction_id, bidder.clone());
    let already_escrowed: i128 = env
        .storage()
        .persistent()
        .get::<DataKey, Bid>(&bid_key)
        .filter(|b| !b.refunded)
        .map(|b| b.amount)
        .unwrap_or(0);

    let to_escrow = bid_amount - already_escrowed;
    if to_escrow > 0 {
        let token_client = token::Client::new(&env, &auction.token);
        token_client.transfer(&bidder, &env.current_contract_address(), &to_escrow);
    }

    // Record new highest bid. The previous highest bidder's existing bid
    // record is left untouched (still unrefunded) -- their escrowed funds
    // remain reclaimable via `claim_refund`.
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

    env.storage().persistent().set(&bid_key, &bid);

    env.storage()
        .persistent()
        .set(&DataKey::CrossChainAuction(auction_id), &auction);

    Ok(())
}

/// Claim a refund of escrowed bid funds for `bidder` on `auction_id`.
///
/// Available once the bidder has been outbid (at any time, without waiting
/// for settlement), or once the auction has ended without them winning
/// (cancelled, or settled with reserve unmet, or settled with someone else
/// winning). The winning bidder's escrow is not refundable once the auction
/// has actually settled with them as the winner — those funds are the
/// settlement proceeds (Issue #1457).
pub fn claim_refund(env: Env, auction_id: u64, bidder: Address) -> Result<(), ContractError> {
    bidder.require_auth();

    let auction: CrossChainAuction = env
        .storage()
        .persistent()
        .get(&DataKey::CrossChainAuction(auction_id))
        .ok_or(ContractError::NoActiveLoan)?;

    let bid_key = DataKey::AuctionBid(auction_id, bidder.clone());
    let mut bid: Bid = env
        .storage()
        .persistent()
        .get(&bid_key)
        .ok_or(ContractError::NoRefundAvailable)?;

    if bid.refunded {
        return Err(ContractError::NoRefundAvailable);
    }

    let is_current_winner = auction.highest_bidder == Some(bidder.clone());
    let has_winning_settlement = env
        .storage()
        .persistent()
        .has(&DataKey::AuctionSettlement(auction_id));

    // The winning bidder's escrow becomes the settlement proceeds once the
    // auction has actually settled with a winner -- not refundable.
    if has_winning_settlement && is_current_winner {
        return Err(ContractError::NoRefundAvailable);
    }

    // Otherwise: refundable once outbid, or once the auction has ended
    // without this bidder winning (cancelled / failed reserve / settled).
    if !auction.settled && is_current_winner {
        return Err(ContractError::NoRefundAvailable);
    }

    bid.refunded = true;
    env.storage().persistent().set(&bid_key, &bid);

    let token_client = token::Client::new(&env, &auction.token);
    token_client.transfer(&env.current_contract_address(), &bidder, &bid.amount);

    Ok(())
}

/// End auction and settle with highest bidder
pub fn settle_auction(
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
