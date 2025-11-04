//! 🔢 Bonding Curve Parser - Extract Price from Pump.fun Accounts
//!
//! Parses Pump.fun bonding curve account data to extract:
//! - Virtual SOL reserves
//! - Virtual token reserves
//! - Current price (SOL per token)
//! - Bonding curve completion status
//!
//! This is used by the gRPC monitor to update mint_cache with real-time prices.

use anyhow::{Context, Result};
use log::{debug, warn};
use solana_sdk::pubkey::Pubkey;

/// Pump.fun bonding curve constants
pub const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
pub const PUMP_CURVE_SEED: &[u8] = b"bonding-curve";
pub const PUMP_CURVE_STATE_SIGNATURE: [u8; 8] = [0x17, 0xb7, 0xf8, 0x37, 0x60, 0xd8, 0xac, 0x60];

// Bonding curve field offsets
const OFFSET_VIRTUAL_TOKEN_RESERVES: usize = 0x08;
const OFFSET_VIRTUAL_SOL_RESERVES: usize = 0x10;
const OFFSET_REAL_TOKEN_RESERVES: usize = 0x18;
const OFFSET_REAL_SOL_RESERVES: usize = 0x20;
const OFFSET_TOKEN_TOTAL_SUPPLY: usize = 0x28;
const OFFSET_COMPLETE: usize = 0x30;

const PUMP_CURVE_TOKEN_DECIMALS: u32 = 6;
const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

/// Bonding curve state extracted from account data
#[derive(Debug, Clone)]
pub struct BondingCurveState {
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub token_total_supply: u64,
    pub complete: bool,
}

impl BondingCurveState {
    /// Parse bonding curve state from raw account data
    pub fn from_account_data(data: &[u8]) -> Result<Self> {
        if data.len() < 49 {
            // Minimum: 8 (sig) + 8 + 8 + 8 + 8 + 8 + 1 = 49 bytes
            anyhow::bail!("Account data too small: {} bytes", data.len());
        }

        // Verify signature
        let signature = &data[0..8];
        if signature != PUMP_CURVE_STATE_SIGNATURE {
            anyhow::bail!("Invalid bonding curve signature: {:?}", signature);
        }

        // Read u64 values (little-endian)
        let virtual_token_reserves = u64::from_le_bytes(
            data[OFFSET_VIRTUAL_TOKEN_RESERVES..OFFSET_VIRTUAL_TOKEN_RESERVES + 8]
                .try_into()
                .context("Invalid virtual_token_reserves")?,
        );

        let virtual_sol_reserves = u64::from_le_bytes(
            data[OFFSET_VIRTUAL_SOL_RESERVES..OFFSET_VIRTUAL_SOL_RESERVES + 8]
                .try_into()
                .context("Invalid virtual_sol_reserves")?,
        );

        let real_token_reserves = u64::from_le_bytes(
            data[OFFSET_REAL_TOKEN_RESERVES..OFFSET_REAL_TOKEN_RESERVES + 8]
                .try_into()
                .context("Invalid real_token_reserves")?,
        );

        let real_sol_reserves = u64::from_le_bytes(
            data[OFFSET_REAL_SOL_RESERVES..OFFSET_REAL_SOL_RESERVES + 8]
                .try_into()
                .context("Invalid real_sol_reserves")?,
        );

        let token_total_supply = u64::from_le_bytes(
            data[OFFSET_TOKEN_TOTAL_SUPPLY..OFFSET_TOKEN_TOTAL_SUPPLY + 8]
                .try_into()
                .context("Invalid token_total_supply")?,
        );

        let complete = data[OFFSET_COMPLETE] != 0;

        Ok(BondingCurveState {
            virtual_token_reserves,
            virtual_sol_reserves,
            real_token_reserves,
            real_sol_reserves,
            token_total_supply,
            complete,
        })
    }

    /// Calculate current token price in SOL
    /// Price = virtual_sol_reserves / virtual_token_reserves
    pub fn calculate_price(&self) -> f64 {
        if self.virtual_token_reserves == 0 || self.virtual_sol_reserves == 0 {
            return 0.0;
        }

        let sol_in_lamports = self.virtual_sol_reserves as f64;
        let tokens_in_base_units = self.virtual_token_reserves as f64;

        // Convert to human-readable units
        let sol_amount = sol_in_lamports / LAMPORTS_PER_SOL as f64;
        let token_amount = tokens_in_base_units / 10_f64.powi(PUMP_CURVE_TOKEN_DECIMALS as i32);

        // Price = SOL per token
        sol_amount / token_amount
    }

    /// Calculate market cap in SOL
    pub fn calculate_market_cap_sol(&self) -> f64 {
        let price = self.calculate_price();
        let total_supply = self.token_total_supply as f64 / 10_f64.powi(PUMP_CURVE_TOKEN_DECIMALS as i32);
        price * total_supply
    }

    /// Get price in lamports per token (for mint_cache)
    pub fn price_lamports_per_token(&self) -> u64 {
        if self.virtual_token_reserves == 0 {
            return 0;
        }
        // lamports per token (in base units)
        self.virtual_sol_reserves / self.virtual_token_reserves
    }
}

/// Calculate bonding curve PDA from mint
pub fn get_bonding_curve_pda(mint: &Pubkey) -> (Pubkey, u8) {
    let pump_program = Pubkey::try_from(PUMP_PROGRAM_ID).expect("Invalid pump program ID");
    Pubkey::find_program_address(&[PUMP_CURVE_SEED, mint.as_ref()], &pump_program)
}

/// Parse account update and extract price
pub fn parse_account_update(account_pubkey: &Pubkey, data: &[u8]) -> Option<(Pubkey, f64, f64)> {
    // Try to parse as bonding curve
    match BondingCurveState::from_account_data(data) {
        Ok(curve) => {
            let price = curve.calculate_price();
            let mc = curve.calculate_market_cap_sol();
            
            debug!(
                "📊 Bonding curve update: {} | price: {:.10} SOL | MC: {:.2} SOL | complete: {}",
                &account_pubkey.to_string()[..12],
                price,
                mc,
                curve.complete
            );

            // We need to derive the mint from the bonding curve PDA
            // For now, return None - caller must maintain bonding_curve -> mint mapping
            // TODO: Add reverse lookup in gRPC monitor
            None
        }
        Err(e) => {
            warn!(
                "⚠️  Failed to parse bonding curve {}: {}",
                &account_pubkey.to_string()[..12],
                e
            );
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_price() {
        let curve = BondingCurveState {
            virtual_token_reserves: 1_000_000_000_000, // 1M tokens (6 decimals)
            virtual_sol_reserves: 30_000_000_000,      // 30 SOL (9 decimals)
            real_token_reserves: 0,
            real_sol_reserves: 0,
            token_total_supply: 1_000_000_000_000,
            complete: false,
        };

        let price = curve.calculate_price();
        // Expected: 30 SOL / 1M tokens = 0.00003 SOL per token
        assert!((price - 0.00003).abs() < 0.0000001);
    }

    #[test]
    fn test_bonding_curve_pda() {
        let mint = Pubkey::new_unique();
        let (pda, bump) = get_bonding_curve_pda(&mint);
        
        // Verify it's a valid PDA (no error)
        assert!(bump < 255);
        
        // Verify deterministic
        let (pda2, bump2) = get_bonding_curve_pda(&mint);
        assert_eq!(pda, pda2);
        assert_eq!(bump, bump2);
    }
}
