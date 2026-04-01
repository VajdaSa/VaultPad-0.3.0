// ─── PDA Seeds ──────────────────────────────────────────────
pub const PROTOCOL_SEED: &[u8] = b"protocol";
pub const LAUNCH_SEED: &[u8] = b"launch";
pub const SOL_VAULT_SEED: &[u8] = b"sol_vault";
pub const CHARITY_SEED: &[u8] = b"charity";
pub const LOCK_SEED: &[u8] = b"lock";
pub const SLOT_TRACKER_SEED: &[u8] = b"slot_tracker";
pub const WALLET_TRACKER_SEED: &[u8] = b"wallet_tracker";

// ─── Fees ───────────────────────────────────────────────────
pub const DEFAULT_PLATFORM_FEE_BPS: u16 = 100;  // 1%
pub const DEFAULT_CREATOR_FEE_BPS: u16 = 100;   // 1%
pub const MAX_FEE_BPS: u16 = 500;

// ─── Launch cost ────────────────────────────────────────────
pub const DEFAULT_LAUNCH_COST: u64 = 2_500_000_000; // 2.5 SOL

// ─── Token ──────────────────────────────────────────────────
pub const TOKEN_DECIMALS: u8 = 6;
pub const TOKEN_SUPPLY: u64 = 1_000_000_000; // 1B

// ─── Max wallet: 4% of total supply ────────────────────────
pub const MAX_WALLET_BPS: u16 = 400; // 4% = 400 basis points

// ─── Creator lock (1-24h randomized) ────────────────────────
pub const CREATOR_LOCK_MIN: i64 = 3600;
pub const CREATOR_LOCK_MAX: i64 = 86400;
pub const CREATOR_LOCK_PERCENT: u64 = 10;

// ─── Normal buyer lock (5-30 min randomized) ────────────────
pub const BUYER_LOCK_MIN: i64 = 300;
pub const BUYER_LOCK_MAX: i64 = 1800;

// ─── PENALTY lock for same-block / same-funder ──────────────
// 72 hours, opened randomly within that window
pub const PENALTY_LOCK_MIN: i64 = 0;        // could unlock immediately (random)
pub const PENALTY_LOCK_MAX: i64 = 259200;   // 72 hours in seconds

// ─── Same-block threshold ───────────────────────────────────
// If this many unique wallets buy in the same slot → all get penalty
pub const SAME_BLOCK_THRESHOLD: u8 = 2;

// ─── Bonding curve ──────────────────────────────────────────
pub const INITIAL_VIRTUAL_SOL: u64 = 500_000_000;

// ─── Categories ─────────────────────────────────────────────
pub const CATEGORY_MEME: u8 = 0;
pub const CATEGORY_TECH: u8 = 1;
pub const CATEGORY_CAUSE: u8 = 2;
pub const CATEGORY_DEFI: u8 = 3;

// ─── String limits ──────────────────────────────────────────
pub const MAX_NAME_LEN: usize = 32;
pub const MAX_TICKER_LEN: usize = 10;
pub const MAX_URI_LEN: usize = 200;
pub const MAX_HANDLE_LEN: usize = 64;
