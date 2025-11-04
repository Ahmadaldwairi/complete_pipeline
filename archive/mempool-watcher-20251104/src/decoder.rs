use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

// Pump.fun instruction discriminators
const BUY_DISCRIMINATOR: [u8; 8] = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea];
const SELL_DISCRIMINATOR: [u8; 8] = [0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad];

// Instruction layout sizes
const BUY_INSTRUCTION_SIZE: usize = 24; // 8 (discriminator) + 8 (amount) + 8 (min_out)
const SELL_INSTRUCTION_SIZE: usize = 24; // 8 (discriminator) + 8 (amount) + 8 (min_sol)

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

/// Parsed Pump.fun BUY instruction
#[derive(Debug, Clone)]
pub struct PumpBuyInstruction {
    pub amount_lamports: u64,
    pub min_tokens_out: u64,
    pub mint: Pubkey,
    pub user: Pubkey,
    pub bonding_curve: Pubkey,
}

/// Parsed Pump.fun SELL instruction
#[derive(Debug, Clone)]
pub struct PumpSellInstruction {
    pub token_amount: u64,
    pub min_sol_out: u64,
    pub mint: Pubkey,
    pub user: Pubkey,
    pub bonding_curve: Pubkey,
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

    /// Parse Pump.fun BUY instruction from raw instruction data
    pub fn parse_pump_buy_instruction(&self, instruction_data: &[u8], accounts: &[Pubkey]) -> Result<Option<PumpBuyInstruction>> {
        if instruction_data.len() < BUY_INSTRUCTION_SIZE {
            return Ok(None);
        }

        // Check discriminator
        let discriminator = &instruction_data[0..8];
        if discriminator != BUY_DISCRIMINATOR {
            return Ok(None);
        }

        // Parse instruction data (little-endian)
        let amount_lamports = u64::from_le_bytes(
            instruction_data[8..16].try_into()
                .context("Failed to parse amount_lamports")?
        );
        
        let min_tokens_out = u64::from_le_bytes(
            instruction_data[16..24].try_into()
                .context("Failed to parse min_tokens_out")?
        );

        // Extract accounts (order as per Pump.fun program)
        if accounts.len() < 6 {
            return Ok(None);
        }

        let user = accounts[0];           // Fee payer/signer
        let mint = accounts[1];           // Token mint
        let bonding_curve = accounts[2];  // Bonding curve account
        // accounts[3] = associated token account
        // accounts[4] = system program
        // accounts[5] = token program

        Ok(Some(PumpBuyInstruction {
            amount_lamports,
            min_tokens_out,
            mint,
            user,
            bonding_curve,
        }))
    }

    /// Parse Pump.fun SELL instruction from raw instruction data
    pub fn parse_pump_sell_instruction(&self, instruction_data: &[u8], accounts: &[Pubkey]) -> Result<Option<PumpSellInstruction>> {
        if instruction_data.len() < SELL_INSTRUCTION_SIZE {
            return Ok(None);
        }

        // Check discriminator
        let discriminator = &instruction_data[0..8];
        if discriminator != SELL_DISCRIMINATOR {
            return Ok(None);
        }

        // Parse instruction data (little-endian)
        let token_amount = u64::from_le_bytes(
            instruction_data[8..16].try_into()
                .context("Failed to parse token_amount")?
        );
        
        let min_sol_out = u64::from_le_bytes(
            instruction_data[16..24].try_into()
                .context("Failed to parse min_sol_out")?
        );

        // Extract accounts
        if accounts.len() < 6 {
            return Ok(None);
        }

        let user = accounts[0];
        let mint = accounts[1];
        let bonding_curve = accounts[2];

        Ok(Some(PumpSellInstruction {
            token_amount,
            min_sol_out,
            mint,
            user,
            bonding_curve,
        }))
    }

    /// Decode Pump.fun instruction into DecodedTransaction
    pub fn decode_pump_instruction(&self, instruction_data: &[u8], accounts: &[Pubkey], timestamp: u64) -> Result<Option<DecodedTransaction>> {
        // Try to parse as BUY instruction
        if let Ok(Some(buy_ix)) = self.parse_pump_buy_instruction(instruction_data, accounts) {
            let amount_sol = buy_ix.amount_lamports as f64 / 1_000_000_000.0; // Convert lamports to SOL
            
            return Ok(Some(DecodedTransaction {
                signature: "unknown".to_string(), // Would be provided from transaction context
                mint: buy_ix.mint.to_string(),
                action: TransactionAction::Buy,
                amount_sol,
                wallet: buy_ix.user.to_string(),
                wallet_type: self.classify_wallet(amount_sol, 0), // Repeat count would come from cache
                timestamp,
                program: ProgramType::PumpFun,
            }));
        }

        // Try to parse as SELL instruction
        if let Ok(Some(sell_ix)) = self.parse_pump_sell_instruction(instruction_data, accounts) {
            let min_sol = sell_ix.min_sol_out as f64 / 1_000_000_000.0; // Convert lamports to SOL
            
            return Ok(Some(DecodedTransaction {
                signature: "unknown".to_string(),
                mint: sell_ix.mint.to_string(),
                action: TransactionAction::Sell,
                amount_sol: min_sol, // Approximation, actual SOL received would be in logs
                wallet: sell_ix.user.to_string(),
                wallet_type: self.classify_wallet(min_sol, 0),
                timestamp,
                program: ProgramType::PumpFun,
            }));
        }

        Ok(None)
    }

    /// Check if instruction data matches Pump.fun BUY discriminator
    pub fn is_pump_buy(&self, instruction_data: &[u8]) -> bool {
        instruction_data.len() >= 8 && &instruction_data[0..8] == BUY_DISCRIMINATOR
    }

    /// Check if instruction data matches Pump.fun SELL discriminator  
    pub fn is_pump_sell(&self, instruction_data: &[u8]) -> bool {
        instruction_data.len() >= 8 && &instruction_data[0..8] == SELL_DISCRIMINATOR
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
