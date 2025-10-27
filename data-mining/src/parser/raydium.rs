use crate::types::{PumpEvent, TradeSide};
use anyhow::{anyhow, Context, Result};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use std::str::FromStr;
use tracing::{debug, info, warn};

// Raydium CPMM program uses instruction indices as discriminators
// From raydium.json instruction order:
const SWAP_BASE_INPUT_IX: u8 = 8;  // swapBaseInput
const SWAP_BASE_OUTPUT_IX: u8 = 9; // swapBaseOutput

pub struct RaydiumParser {
    raydium_program_id: Pubkey,
}

impl RaydiumParser {
    pub fn new(raydium_program_id: &str) -> Result<Self> {
        let pubkey = Pubkey::from_str(raydium_program_id)
            .map_err(|e| anyhow!("Invalid Raydium program ID: {}", e))?;

        Ok(Self {
            raydium_program_id: pubkey,
        })
    }

    /// Check if a transaction contains Raydium swap instructions
    pub fn parse_transaction(
        &self,
        tx: &yellowstone_grpc_proto::prelude::SubscribeUpdateTransactionInfo,
        slot: u64,
        block_time: i64,
    ) -> Result<Vec<PumpEvent>> {
        let mut events = Vec::new();

        // Get transaction signature
        let signature = if let Some(tx_info) = &tx.transaction {
            if let Some(first_sig) = tx_info.signatures.first() {
                if first_sig.len() >= 64 {
                    Signature::try_from(&first_sig[0..64])
                        .context("Failed to parse signature")?
                } else {
                    return Ok(events);
                }
            } else {
                return Ok(events);
            }
        } else {
            return Ok(events);
        };

        if let Some(tx_info) = &tx.transaction {
            if let Some(meta) = &tx.meta {
                if let Some(tx_msg) = &tx_info.message {
                    let account_keys: Vec<String> = tx_msg.account_keys.iter()
                        .map(|k| bs58::encode(k).into_string())
                        .collect();
                    
                    // Check inner instructions (most swaps happen here)
                    debug!("🔍 Checking {} inner instruction sets for Raydium", meta.inner_instructions.len());
                    for inner_ix_set in &meta.inner_instructions {
                        for inner_ix in &inner_ix_set.instructions {
                            let program_idx = inner_ix.program_id_index as usize;
                            if program_idx < account_keys.len() {
                                if let Ok(program_pubkey) = Pubkey::from_str(&account_keys[program_idx]) {
                                    if program_pubkey == self.raydium_program_id {
                                        info!("🌊 Found Raydium instruction in INNER instructions!");
                                        if let Some(event) = self.parse_swap_instruction(
                                            &inner_ix.data,
                                            &account_keys,
                                            inner_ix.accounts.iter().map(|a| *a as usize).collect(),
                                            &signature,
                                            slot,
                                            block_time,
                                        )? {
                                            info!("✅ Successfully parsed Raydium swap!");
                                            events.push(event);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Check top-level instructions
                    let instructions = &tx_msg.instructions;
                    if !instructions.is_empty() {
                        debug!("🔍 Checking {} top-level instructions for Raydium", instructions.len());
                        for ix in instructions {
                            let program_idx = ix.program_id_index as usize;
                            if program_idx < account_keys.len() {
                                if let Ok(program_pubkey) = Pubkey::from_str(&account_keys[program_idx]) {
                                    if program_pubkey == self.raydium_program_id {
                                        debug!("🌊 Found Raydium instruction in top-level instructions");
                                        if let Some(event) = self.parse_swap_instruction(
                                            &ix.data,
                                            &account_keys,
                                            ix.accounts.iter().map(|a| *a as usize).collect(),
                                            &signature,
                                            slot,
                                            block_time,
                                        )? {
                                            debug!("✅ Successfully parsed Raydium swap!");
                                            events.push(event);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(events)
    }

    fn parse_swap_instruction(
        &self,
        instruction_data: &[u8],
        account_keys: &[String],
        accounts: Vec<usize>,
        signature: &Signature,
        slot: u64,
        block_time: i64,
    ) -> Result<Option<PumpEvent>> {
        if instruction_data.is_empty() {
            debug!("Empty instruction data");
            return Ok(None);
        }

        // Raydium uses instruction index as first byte (not 8-byte discriminator)
        let instruction_idx = instruction_data[0];
        let data = &instruction_data[1..];

        match instruction_idx {
            SWAP_BASE_INPUT_IX => {
                info!("🌊 Parsing Raydium swapBaseInput");
                self.parse_swap_base_input(data, account_keys, accounts, signature, slot, block_time)
            }
            SWAP_BASE_OUTPUT_IX => {
                info!("🌊 Parsing Raydium swapBaseOutput");
                self.parse_swap_base_output(data, account_keys, accounts, signature, slot, block_time)
            }
            _ => {
                debug!("Unknown Raydium instruction index: {}", instruction_idx);
                Ok(None)
            }
        }
    }

    fn parse_swap_base_input(
        &self,
        data: &[u8],
        account_keys: &[String],
        accounts: Vec<usize>,
        signature: &Signature,
        slot: u64,
        block_time: i64,
    ) -> Result<Option<PumpEvent>> {
        // swapBaseInput args from IDL:
        // - amount_in: u64
        // - minimum_amount_out: u64
        
        if data.len() < 16 {
            warn!("swapBaseInput data too short");
            return Ok(None);
        }

        let amount_in = u64::from_le_bytes(data[0..8].try_into()?);
        let minimum_amount_out = u64::from_le_bytes(data[8..16].try_into()?);

        // swapBaseInput accounts from IDL (in order):
        // 0: payer (user - signer)
        // 1: authority
        // 2: ammConfig
        // 3: poolState
        // 4: inputTokenAccount
        // 5: outputTokenAccount
        // 6: inputVault
        // 7: outputVault
        // 8: inputTokenProgram
        // 9: outputTokenProgram
        // 10: inputTokenMint
        // 11: outputTokenMint
        // 12: observationState
        
        if accounts.len() < 12 {
            warn!("swapBaseInput: not enough accounts (need 12, got {})", accounts.len());
            return Ok(None);
        }

        let user = &account_keys[accounts[0]];
        let pool_state = &account_keys[accounts[3]];
        let input_mint = if accounts.len() > 10 { &account_keys[accounts[10]] } else { "unknown" };
        let output_mint = if accounts.len() > 11 { &account_keys[accounts[11]] } else { "unknown" };

        info!("🌊 Raydium SWAP detected: user={}, pool={}, amount_in={}, min_out={}", 
            user, pool_state, amount_in, minimum_amount_out);
        info!("   Input mint: {}, Output mint: {}", input_mint, output_mint);

        // We need to determine if this is a BUY or SELL based on the mints
        // For graduated pump.fun tokens, we need to check if input_mint or output_mint
        // matches a known pump.fun token
        // For now, just log the swap - the actual Trade event will be created when
        // we can identify the token

        // Return None for now - we'll enhance this when we have token identification
        Ok(None)
    }

    fn parse_swap_base_output(
        &self,
        data: &[u8],
        account_keys: &[String],
        accounts: Vec<usize>,
        signature: &Signature,
        slot: u64,
        block_time: i64,
    ) -> Result<Option<PumpEvent>> {
        // swapBaseOutput args from IDL:
        // - max_amount_in: u64
        // - amount_out: u64
        
        if data.len() < 16 {
            warn!("swapBaseOutput data too short");
            return Ok(None);
        }

        let max_amount_in = u64::from_le_bytes(data[0..8].try_into()?);
        let amount_out = u64::from_le_bytes(data[8..16].try_into()?);

        // Same account structure as swapBaseInput
        if accounts.len() < 12 {
            warn!("swapBaseOutput: not enough accounts (need 12, got {})", accounts.len());
            return Ok(None);
        }

        let user = &account_keys[accounts[0]];
        let pool_state = &account_keys[accounts[3]];
        let input_mint = if accounts.len() > 10 { &account_keys[accounts[10]] } else { "unknown" };
        let output_mint = if accounts.len() > 11 { &account_keys[accounts[11]] } else { "unknown" };

        info!("🌊 Raydium SWAP detected: user={}, pool={}, max_in={}, amount_out={}", 
            user, pool_state, max_amount_in, amount_out);
        info!("   Input mint: {}, Output mint: {}", input_mint, output_mint);

        // Return None for now - same reason as swapBaseInput
        Ok(None)
    }
}
