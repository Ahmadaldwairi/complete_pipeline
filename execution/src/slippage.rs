//! 📊 Slippage Calculator
//! 
//! Calculates actual slippage by comparing simulated vs realized execution.
//! This is more accurate than mid-price comparisons as it accounts for:
//! - Actual bonding curve state at execution time
//! - MEV/frontrunning impact on position
//! - Network latency effects

use anyhow::{Context, Result};
use log::{debug, info, warn};
use solana_client::rpc_client::RpcClient;
use solana_sdk::signature::Signature;
use solana_client::rpc_config::RpcTransactionConfig;
use solana_transaction_status::{
    UiTransactionEncoding,
    UiInstruction,
    option_serializer::OptionSerializer,
};

/// Slippage calculation result
#[derive(Debug, Clone)]
pub struct SlippageResult {
    /// Expected tokens/SOL from simulation
    pub expected_amount: f64,
    
    /// Actual tokens/SOL from transaction
    pub actual_amount: f64,
    
    /// Slippage percentage: (expected - actual) / expected * 100
    /// Positive = worse than expected (slippage loss)
    /// Negative = better than expected (slippage gain - rare but possible)
    pub slippage_pct: f64,
    
    /// Slippage in basis points (for database storage)
    pub slippage_bps: i32,
    
    /// Whether slippage exceeded common thresholds
    pub exceeded_1pct: bool,
    pub exceeded_5pct: bool,
}

impl SlippageResult {
    pub fn new(expected: f64, actual: f64) -> Self {
        let slippage_pct = if expected > 0.0 {
            ((expected - actual) / expected) * 100.0
        } else {
            0.0
        };
        
        let slippage_bps = (slippage_pct * 100.0) as i32;
        
        Self {
            expected_amount: expected,
            actual_amount: actual,
            slippage_pct,
            slippage_bps,
            exceeded_1pct: slippage_pct.abs() > 1.0,
            exceeded_5pct: slippage_pct.abs() > 5.0,
        }
    }
    
    pub fn log(&self, side: &str) {
        let emoji = if self.slippage_pct > 0.0 { "📉" } else { "📈" };
        let direction = if self.slippage_pct > 0.0 { "LOSS" } else { "GAIN" };
        
        info!("{} {} Slippage Analysis:", emoji, side);
        info!("   Expected: {:.6}", self.expected_amount);
        info!("   Actual: {:.6}", self.actual_amount);
        info!("   Slippage: {:.2}% ({} bps) [{}]", 
            self.slippage_pct, self.slippage_bps, direction);
        
        if self.exceeded_5pct {
            warn!("⚠️  HIGH SLIPPAGE: Exceeded 5% threshold!");
        } else if self.exceeded_1pct {
            warn!("⚠️  Moderate slippage: Exceeded 1% threshold");
        }
    }
}

/// Parse actual token amount from Pump.fun buy transaction
/// 
/// Pump.fun buy transfers tokens to buyer's ATA via inner instructions.
/// We parse the token transfer amount from the SPL Token program inner instruction.
pub async fn parse_actual_tokens_from_buy(
    rpc_client: &RpcClient,
    signature: &Signature,
) -> Result<f64> {
    let tx_meta = fetch_transaction_meta(rpc_client, signature).await?;
    
    // Look for SPL Token Transfer in inner instructions
    // Pump.fun structure: outer instruction calls pump program,
    // which internally calls SPL Token to transfer tokens to buyer
    
    if let OptionSerializer::Some(inner_instructions) = &tx_meta.inner_instructions {
        for inner_group in inner_instructions {
            for instruction in &inner_group.instructions {
                // SPL Token Transfer instruction has specific structure
                // Check if it's a Transfer instruction (instruction index 3 or 12 for TransferChecked)
                // UiInstruction is an enum, match on Parsed variant
                if let UiInstruction::Parsed(parsed_instr) = instruction {
                    // Convert UiParsedInstruction to JSON for flexible parsing
                    if let Ok(parsed_json) = serde_json::to_value(parsed_instr) {
                        if let Some(type_) = parsed_json.get("type").and_then(|t| t.as_str()) {
                            if type_ == "transfer" || type_ == "transferChecked" {
                                // Extract token amount
                                if let Some(info) = parsed_json.get("info") {
                                    let amount = if type_ == "transferChecked" {
                                        info.get("tokenAmount")
                                            .and_then(|ta| ta.get("uiAmount"))
                                            .and_then(|v| v.as_f64())
                                    } else {
                                        info.get("amount")
                                            .and_then(|v| v.as_str())
                                            .and_then(|s| s.parse::<f64>().ok())
                                            .map(|raw| raw / 1_000_000.0) // Pump.fun tokens have 6 decimals
                                    };
                                    
                                    if let Some(amt) = amount {
                                        debug!("📦 Found token transfer: {:.6} tokens", amt);
                                        return Ok(amt);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    Err(anyhow::anyhow!("No token transfer found in transaction inner instructions"))
}

/// Parse actual SOL received from Pump.fun sell transaction
/// 
/// Pump.fun sell transfers SOL from bonding curve to seller.
/// We parse the SOL transfer amount from inner instructions or balance changes.
pub async fn parse_actual_sol_from_sell(
    rpc_client: &RpcClient,
    signature: &Signature,
    seller_pubkey: &solana_sdk::pubkey::Pubkey,
) -> Result<f64> {
    let tx_meta = fetch_transaction_meta(rpc_client, signature).await?;
    
    // Method 1: Parse from inner instructions (most accurate)
    if let OptionSerializer::Some(inner_instructions) = &tx_meta.inner_instructions {
        for inner_group in inner_instructions {
            for instruction in &inner_group.instructions {
                if let UiInstruction::Parsed(parsed_instr) = instruction {
                    // Convert UiParsedInstruction to JSON for flexible parsing
                    if let Ok(parsed_json) = serde_json::to_value(parsed_instr) {
                        if let Some(type_) = parsed_json.get("type").and_then(|t| t.as_str()) {
                            if type_ == "transfer" {
                                if let Some(info) = parsed_json.get("info") {
                                    // Check if destination is our wallet
                                    if let Some(destination) = info.get("destination").and_then(|d| d.as_str()) {
                                        if destination == seller_pubkey.to_string() {
                                            if let Some(lamports) = info.get("lamports").and_then(|l| l.as_u64()) {
                                                let sol_amount = lamports as f64 / 1_000_000_000.0;
                                                debug!("💰 Found SOL transfer: {:.6} SOL", sol_amount);
                                                return Ok(sol_amount);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Method 2: Calculate from balance changes (fallback)
    if let (Some(pre_balances), Some(post_balances)) = 
        (tx_meta.pre_balances.first(), tx_meta.post_balances.first()) 
    {
        let balance_change = (*post_balances as i64 - *pre_balances as i64) as f64 / 1_000_000_000.0;
        
        // For sells, we expect positive balance change (received SOL)
        // Subtract transaction fee to get actual proceeds
        let fee_sol = tx_meta.fee as f64 / 1_000_000_000.0;
        let sol_received = balance_change + fee_sol;
        
        if sol_received > 0.0 {
            debug!("💰 Calculated SOL from balance: {:.6} SOL (change: {:.6}, fee: {:.6})", 
                sol_received, balance_change, fee_sol);
            return Ok(sol_received);
        }
    }
    
    Err(anyhow::anyhow!("Could not parse SOL amount from sell transaction"))
}

/// Fetch transaction metadata with inner instructions
async fn fetch_transaction_meta(
    rpc_client: &RpcClient,
    signature: &Signature,
) -> Result<solana_transaction_status::UiTransactionStatusMeta> {
    let config = RpcTransactionConfig {
        encoding: Some(UiTransactionEncoding::JsonParsed), // JsonParsed for readable inner instructions
        commitment: Some(solana_sdk::commitment_config::CommitmentConfig::confirmed()),
        max_supported_transaction_version: Some(0),
    };
    
    let tx = rpc_client.get_transaction_with_config(signature, config)
        .context("Failed to fetch transaction")?;
    
    tx.transaction.meta
        .ok_or_else(|| anyhow::anyhow!("Transaction meta not available"))
}

/// Calculate buy slippage (token amount perspective)
/// 
/// Simulates expected tokens from bonding curve, then compares with actual tokens received.
pub async fn calculate_buy_slippage(
    rpc_client: &RpcClient,
    signature: &Signature,
    expected_tokens: f64,
) -> Result<SlippageResult> {
    let actual_tokens = parse_actual_tokens_from_buy(rpc_client, signature).await?;
    Ok(SlippageResult::new(expected_tokens, actual_tokens))
}

/// Calculate sell slippage (SOL amount perspective)
/// 
/// Simulates expected SOL from bonding curve, then compares with actual SOL received.
pub async fn calculate_sell_slippage(
    rpc_client: &RpcClient,
    signature: &Signature,
    seller_pubkey: &solana_sdk::pubkey::Pubkey,
    expected_sol: f64,
) -> Result<SlippageResult> {
    let actual_sol = parse_actual_sol_from_sell(rpc_client, signature, seller_pubkey).await?;
    Ok(SlippageResult::new(expected_sol, actual_sol))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_slippage_calculation() {
        // Test case 1: Normal slippage loss
        let result = SlippageResult::new(1000.0, 985.0);
        assert!((result.slippage_pct - 1.5).abs() < 0.01);
        assert_eq!(result.slippage_bps, 150);
        assert!(result.exceeded_1pct);
        assert!(!result.exceeded_5pct);
        
        // Test case 2: Slippage gain (better than expected)
        let result = SlippageResult::new(1000.0, 1010.0);
        assert!(result.slippage_pct < 0.0);
        assert_eq!(result.slippage_bps, -100);
        
        // Test case 3: High slippage
        let result = SlippageResult::new(1000.0, 920.0);
        assert!((result.slippage_pct - 8.0).abs() < 0.01);
        assert!(result.exceeded_5pct);
        
        // Test case 4: Minimal slippage
        let result = SlippageResult::new(1000.0, 999.0);
        assert!((result.slippage_pct - 0.1).abs() < 0.01);
        assert!(!result.exceeded_1pct);
    }
}
