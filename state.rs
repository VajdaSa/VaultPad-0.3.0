use anchor_lang::prelude::*;
use crate::errors::VaultpadError;

/// Global config. PDA: [b"protocol"]
#[account]
#[derive(InitSpace)]
pub struct ProtocolConfig {
    pub authority: Pubkey,
    pub platform_wallet: Pubkey,
    pub platform_fee_bps: u16,
    pub creator_fee_bps: u16,
    pub launch_cost: u64,
    pub total_launches: u64,
    pub total_volume: u64,
    pub total_fees_collected: u64,
    pub bump: u8,
    pub _reserved: [u8; 64],
}

/// Per-launch state. PDA: [b"launch", mint]
#[account]
#[derive(InitSpace)]
pub struct LaunchState {
    pub mint: Pubkey,
    pub creator: Pubkey,
    #[max_len(32)]
    pub name: String,
    #[max_len(10)]
    pub ticker: String,
    #[max_len(200)]
    pub uri: String,
    pub category: u8,
    pub fee_redirect_type: u8,
    #[max_len(64)]
    pub fee_redirect_handle: String,

    // ─── Bonding curve (xy=k) ───────────────────────────────
    pub virtual_sol_reserves: u64,
    pub virtual_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub graduated: bool,

    /// Total supply in base units (for max wallet calc)
    pub total_supply: u64,

    // ─── Creator lock ───────────────────────────────────────
    pub creator_lock_unlock_time: i64,
    pub creator_lock_claimed: bool,
    pub creator_locked_tokens: u64,

    // ─── Charity (cause only) ───────────────────────────────
    pub charity_wallet: Pubkey,
    pub charity_authority: Pubkey,

    // ─── Stats ──────────────────────────────────────────────
    pub volume: u64,
    pub fees_generated: u64,
    pub trade_count: u64,
    pub total_locks_created: u64,
    pub penalty_locks_issued: u64,
    pub created_at: i64,

    /// Last slot that had a buy (for same-block detection)
    pub last_buy_slot: u64,
    /// Number of unique buyers in `last_buy_slot`
    pub buys_in_current_slot: u8,

    pub bump: u8,
    pub sol_vault_bump: u8,
    pub _reserved: [u8; 32],
}

impl LaunchState {
    pub fn calculate_buy(&self, sol_in: u64) -> Result<u64> {
        let k = (self.virtual_sol_reserves as u128)
            .checked_mul(self.virtual_token_reserves as u128)
            .ok_or(VaultpadError::MathOverflow)?;
        let new_sol = (self.virtual_sol_reserves as u128)
            .checked_add(sol_in as u128)
            .ok_or(VaultpadError::MathOverflow)?;
        let new_tokens = k.checked_div(new_sol)
            .ok_or(VaultpadError::MathOverflow)?;
        let tokens_out = (self.virtual_token_reserves as u128)
            .checked_sub(new_tokens)
            .ok_or(VaultpadError::CurveDepleted)?;
        Ok(tokens_out.min(self.real_token_reserves as u128) as u64)
    }

    pub fn calculate_sell(&self, tokens_in: u64) -> Result<u64> {
        let k = (self.virtual_sol_reserves as u128)
            .checked_mul(self.virtual_token_reserves as u128)
            .ok_or(VaultpadError::MathOverflow)?;
        let new_tokens = (self.virtual_token_reserves as u128)
            .checked_add(tokens_in as u128)
            .ok_or(VaultpadError::MathOverflow)?;
        let new_sol = k.checked_div(new_tokens)
            .ok_or(VaultpadError::MathOverflow)?;
        let sol_out = (self.virtual_sol_reserves as u128)
            .checked_sub(new_sol)
            .ok_or(VaultpadError::CurveDepleted)?;
        Ok(sol_out.min(self.real_sol_reserves as u128) as u64)
    }

    /// Calculate max tokens any single wallet can hold (4% of total supply)
    pub fn max_wallet_tokens(&self) -> u64 {
        (self.total_supply as u128)
            .checked_mul(crate::constants::MAX_WALLET_BPS as u128)
            .unwrap()
            .checked_div(10_000)
            .unwrap() as u64
    }
}

/// Per-buyer lock escrow.
/// PDA: [b"lock", mint, buyer, lock_id (u64 LE)]
///
/// Every buy creates one. Tokens sit in vault until buyer claims
/// after unlock_time. Penalty locks (same-block/same-funder)
/// have much longer durations (up to 72h).
#[account]
#[derive(InitSpace)]
pub struct BuyerLock {
    pub mint: Pubkey,
    pub buyer: Pubkey,
    pub lock_id: u64,
    pub amount: u64,
    pub unlock_time: i64,
    pub claimed: bool,
    pub created_at: i64,
    /// Whether this lock was issued as a penalty (same-block or same-funder)
    pub is_penalty: bool,
    pub bump: u8,
}

/// Tracks per-wallet holdings and funder for a specific launch.
/// PDA: [b"wallet_tracker", mint, wallet]
///
/// Used to enforce:
/// 1. Max 4% wallet cap
/// 2. Same-funder detection (penalty lock)
#[account]
#[derive(InitSpace)]
pub struct WalletTracker {
    pub mint: Pubkey,
    pub wallet: Pubkey,
    /// Total tokens this wallet has purchased (across all locks, claimed or not)
    pub total_purchased: u64,
    /// Total tokens currently in active (unclaimed) locks
    pub locked_balance: u64,
    /// Total tokens claimed (in wallet)
    pub claimed_balance: u64,
    /// The account that funded this wallet (first SOL sender).
    /// Set on first buy. Used for same-funder detection.
    pub funder: Pubkey,
    /// Whether this wallet has been flagged for penalty locks
    pub flagged: bool,
    /// Number of buys
    pub buy_count: u32,
    pub bump: u8,
}
