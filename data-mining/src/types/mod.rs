use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub mint: String,
    pub creator_wallet: String,
    pub bonding_curve_addr: Option<String>,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub uri: Option<String>,
    pub decimals: u8,
    pub launch_tx_sig: String,
    pub launch_slot: u64,
    pub launch_block_time: i64,
    pub initial_price: Option<f64>,
    pub initial_liquidity_sol: Option<f64>,
    pub initial_supply: Option<String>,
    pub market_cap_init: Option<f64>,
    pub mint_authority: Option<String>,
    pub freeze_authority: Option<String>,
    pub metadata_update_auth: Option<String>,
    pub migrated_to_raydium: bool,
    pub migration_slot: Option<u64>,
    pub migration_block_time: Option<i64>,
    pub raydium_pool: Option<String>,
    pub observed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub sig: String,
    pub slot: u64,
    pub block_time: i64,
    pub mint: String,
    pub side: TradeSide,
    pub trader: String,
    pub amount_tokens: f64,
    pub amount_sol: f64,
    pub price: f64,
    pub is_amm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TradeSide {
    Buy,
    Sell,
}

impl TradeSide {
    pub fn as_str(&self) -> &str {
        match self {
            TradeSide::Buy => "buy",
            TradeSide::Sell => "sell",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    pub mint: String,
    pub window_sec: u64,
    pub start_slot: u64,
    pub start_time: i64,
    pub end_time: i64,
    pub num_buys: u64,
    pub num_sells: u64,
    pub uniq_buyers: u64,
    pub vol_tokens: f64,
    pub vol_sol: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub vwap: f64,
    pub top1_share: f64,
    pub top3_share: f64,
    pub top5_share: f64,
    pub price_volatility: f64,  // Standard deviation of price
    pub open: f64,              // First price in window
}

#[derive(Debug, Clone)]
pub enum PumpEvent {
    Launch {
        mint: String,
        creator: String,
        bonding_curve: String,
        name: String,
        symbol: String,
        uri: String,
        slot: u64,
        block_time: i64,
        signature: String,
    },
    Trade {
        signature: String,
        slot: u64,
        block_time: i64,
        mint: String,
        side: TradeSide,
        trader: String,
        amount_tokens: u64,
        amount_sol: u64,
        price: f64,
        is_amm: bool,
        virtual_sol_reserves: u64,
        virtual_token_reserves: u64,
    },
    Migrated {
        mint: String,
        pool: String,
        slot: u64,
        block_time: i64,
        signature: String,
    },
}
