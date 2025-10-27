use anyhow::Result;
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

/// Decoded transaction data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecodedTransaction {
    pub signature: String,
    pub mint: String,
    pub action: TransactionAction,
    pub amount_sol: f64,
    pub wallet: String,
    pub wallet_type: WalletType,
    pub timestamp: u64,
    pub program: ProgramType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum TransactionAction {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum WalletType {
    Whale,    // Large holder (>= threshold)
    Bot,      // Repeat trader (detected pattern)
    Retail,   // Regular trader
    Unknown,  // Not yet classified
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum ProgramType {
    PumpFun,
    Raydium,
    Unknown,
}

/// Transaction decoder - parses Pump.fun and Raydium transactions
pub struct TransactionDecoder {
    pump_program_id: Pubkey,
    raydium_program_id: Pubkey,
    whale_threshold_sol: f64,
}

impl TransactionDecoder {
    pub fn new(whale_threshold_sol: f64) -> Self {
        // Pump.fun program ID
        let pump_program_id = Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P")
            .expect("Invalid Pump.fun program ID");

        // Raydium AMM program ID
        let raydium_program_id = Pubkey::from_str("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8")
            .expect("Invalid Raydium program ID");

        Self {
            pump_program_id,
            raydium_program_id,
            whale_threshold_sol,
        }
    }

    /// Decode a transaction
    pub fn decode(&self, _transaction: &[u8]) -> Result<Option<DecodedTransaction>> {
        // TODO: Implement actual transaction parsing
        // For now, return None until we integrate with RPC
        Ok(None)
    }

    /// Identify program type from instruction
    pub fn identify_program(&self, program_id: &Pubkey) -> ProgramType {
        if program_id == &self.pump_program_id {
            ProgramType::PumpFun
        } else if program_id == &self.raydium_program_id {
            ProgramType::Raydium
        } else {
            ProgramType::Unknown
        }
    }

    /// Classify wallet type based on transaction amount
    pub fn classify_wallet(&self, amount_sol: f64, repeat_count: usize) -> WalletType {
        if amount_sol >= self.whale_threshold_sol {
            WalletType::Whale
        } else if repeat_count >= 3 {
            WalletType::Bot
        } else {
            WalletType::Retail
        }
    }

    /// Extract mint address from transaction (stub)
    pub fn extract_mint(&self, _transaction: &[u8]) -> Option<String> {
        // TODO: Parse transaction accounts to find mint
        None
    }

    /// Extract wallet address from transaction (stub)
    pub fn extract_wallet(&self, _transaction: &[u8]) -> Option<String> {
        // TODO: Parse transaction signers to find wallet
        None
    }

    /// Determine if transaction is a buy or sell (stub)
    pub fn determine_action(&self, _transaction: &[u8]) -> Option<TransactionAction> {
        // TODO: Parse instruction data to determine action
        None
    }

    /// Extract SOL amount from transaction (stub)
    pub fn extract_amount(&self, _transaction: &[u8]) -> Option<f64> {
        // TODO: Parse instruction data for amount
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_program_identification() {
        let decoder = TransactionDecoder::new(10.0);
        
        let pump_id = Pubkey::from_str("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P").unwrap();
        assert_eq!(decoder.identify_program(&pump_id), ProgramType::PumpFun);
        
        let raydium_id = Pubkey::from_str("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8").unwrap();
        assert_eq!(decoder.identify_program(&raydium_id), ProgramType::Raydium);
    }

    #[test]
    fn test_wallet_classification() {
        let decoder = TransactionDecoder::new(10.0);

        // Whale (>= threshold)
        assert_eq!(decoder.classify_wallet(15.0, 0), WalletType::Whale);
        
        // Bot (repeat trader)
        assert_eq!(decoder.classify_wallet(5.0, 5), WalletType::Bot);
        
        // Retail (normal)
        assert_eq!(decoder.classify_wallet(2.0, 1), WalletType::Retail);
    }
}
