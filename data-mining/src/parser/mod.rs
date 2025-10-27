use crate::types::{PumpEvent, TradeSide};
use anyhow::{anyhow, Context, Result};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use std::str::FromStr;
use tracing::{debug, info, warn};
use base64::{Engine as _, engine::general_purpose};

// Instruction discriminators from pump.fun IDL
const CREATE_DISCRIMINATOR: [u8; 8] = [24, 30, 200, 40, 5, 28, 7, 119];
const BUY_DISCRIMINATOR: [u8; 8] = [102, 6, 61, 18, 1, 218, 235, 234];
const SELL_DISCRIMINATOR: [u8; 8] = [51, 230, 133, 164, 1, 127, 131, 173];
const MIGRATE_DISCRIMINATOR: [u8; 8] = [155, 234, 231, 146, 236, 158, 162, 30];

// Event discriminators from pump.fun IDL
const CREATE_EVENT_DISCRIMINATOR: [u8; 8] = [27, 114, 169, 77, 222, 235, 99, 118];
const TRADE_EVENT_DISCRIMINATOR: [u8; 8] = [189, 219, 127, 211, 78, 230, 97, 238];
const COMPLETE_EVENT_DISCRIMINATOR: [u8; 8] = [95, 114, 97, 156, 212, 46, 152, 8];
const COMPLETE_PUMP_AMM_MIGRATION_EVENT_DISCRIMINATOR: [u8; 8] =
    [189, 233, 93, 185, 92, 148, 234, 148];

pub struct PumpParser {
    pump_program_id: Pubkey,
}

impl PumpParser {
    pub fn new(pump_program_id: &str) -> Result<Self> {
        let pubkey = Pubkey::from_str(pump_program_id)
            .map_err(|e| anyhow!("Invalid pump program ID: {}", e))?;

        Ok(Self {
            pump_program_id: pubkey,
        })
    }

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

        // STEP 1: Parse event logs (existing method - most reliable)
        if let Some(tx_info) = &tx.transaction {
            if let Some(meta) = &tx.meta {
                debug!("🔍 Checking {} log messages", meta.log_messages.len());
                for (i, log) in meta.log_messages.iter().enumerate() {
                    if log.contains("Program data: ") {
                        debug!("🎯 Found 'Program data:' in log {}", i);
                        if let Some(event_data) = self.extract_event_data(log) {
                            debug!("📦 Extracted {} bytes of event data", event_data.len());
                            if let Some(event) = self.parse_event(&event_data, &signature, slot, block_time)? {
                                debug!("✅ Successfully parsed event from logs!");
                                events.push(event);
                            }
                        }
                    }
                }

                // STEP 2: Check inner instructions (NEW - catches missed BUYs/SELLs)
                // This is where many transactions hide!
                if let Some(tx_msg) = &tx_info.message {
                    let account_keys: Vec<String> = tx_msg.account_keys.iter()
                        .map(|k| bs58::encode(k).into_string())
                        .collect();
                    
                    debug!("🔍 Checking {} inner instruction sets", meta.inner_instructions.len());
                    for inner_ix_set in &meta.inner_instructions {
                        for inner_ix in &inner_ix_set.instructions {
                            let program_idx = inner_ix.program_id_index as usize;
                            if program_idx < account_keys.len() {
                                if let Ok(program_pubkey) = Pubkey::from_str(&account_keys[program_idx]) {
                                    if program_pubkey == self.pump_program_id {
                                        info!("🔍 Found Pump.fun instruction in INNER instructions!");
                                        if let Some(event) = self.parse_instruction(
                                            &inner_ix.data,
                                            &account_keys,
                                            inner_ix.accounts.iter().map(|a| *a as usize).collect(),
                                            &signature,
                                            slot,
                                            block_time,
                                        )? {
                                            info!("✅ Successfully parsed event from inner instruction!");
                                            events.push(event);
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // STEP 3: Check top-level instructions (fallback)
                    let instructions = &tx_msg.instructions;
                    if !instructions.is_empty() {
                        debug!("🔍 Checking {} top-level instructions", instructions.len());
                        for ix in instructions {
                            let program_idx = ix.program_id_index as usize;
                            if program_idx < account_keys.len() {
                                if let Ok(program_pubkey) = Pubkey::from_str(&account_keys[program_idx]) {
                                    if program_pubkey == self.pump_program_id {
                                        debug!("🔍 Found Pump.fun instruction in top-level instructions");
                                        if let Some(event) = self.parse_instruction(
                                            &ix.data,
                                            &account_keys,
                                            ix.accounts.iter().map(|a| *a as usize).collect(),
                                            &signature,
                                            slot,
                                            block_time,
                                        )? {
                                            debug!("✅ Successfully parsed event from top-level instruction!");
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

    /// NEW: Parse instruction data directly (for inner instructions)
    fn parse_instruction(
        &self,
        instruction_data: &[u8],
        account_keys: &[String],
        accounts: Vec<usize>,
        signature: &Signature,
        slot: u64,
        block_time: i64,
    ) -> Result<Option<PumpEvent>> {
        if instruction_data.len() < 8 {
            debug!("Instruction data too short: {} bytes", instruction_data.len());
            return Ok(None);
        }

        let discriminator = &instruction_data[0..8];
        let data = &instruction_data[8..];

        match discriminator {
            disc if disc == BUY_DISCRIMINATOR => {
                info!("🛒 Parsing BUY instruction from inner instructions");
                self.parse_buy_instruction(data, account_keys, accounts, signature, slot, block_time)
            }
            disc if disc == SELL_DISCRIMINATOR => {
                info!("💰 Parsing SELL instruction from inner instructions");
                self.parse_sell_instruction(data, account_keys, accounts, signature, slot, block_time)
            }
            disc if disc == CREATE_DISCRIMINATOR => {
                info!("✨ Parsing CREATE instruction");
                self.parse_create_instruction(data, account_keys, accounts, signature, slot, block_time)
            }
            disc if disc == MIGRATE_DISCRIMINATOR => {
                info!("🚀 Parsing MIGRATE instruction");
                Ok(None) // Migration events are better parsed from logs
            }
            _ => {
                debug!("Unknown instruction discriminator: {:?}", discriminator);
                Ok(None)
            }
        }
    }

    /// Parse BUY instruction from accounts and data
    fn parse_buy_instruction(
        &self,
        data: &[u8],
        account_keys: &[String],
        accounts: Vec<usize>,
        signature: &Signature,
        slot: u64,
        block_time: i64,
    ) -> Result<Option<PumpEvent>> {
        // BUY instruction args from IDL:
        // - amount: u64 (token amount to buy)
        // - max_sol_cost: u64 (slippage protection)
        
        if data.len() < 16 {
            warn!("BUY instruction data too short");
            return Ok(None);
        }

        let token_amount = u64::from_le_bytes(data[0..8].try_into()?);
        let max_sol_cost = u64::from_le_bytes(data[8..16].try_into()?);

        // BUY instruction accounts from IDL (in order):
        // 0: global, 1: feeRecipient, 2: mint, 3: bondingCurve, 
        // 4: associatedBondingCurve, 5: associatedUser, 6: user (signer)
        
        if accounts.len() < 7 {
            warn!("BUY instruction: not enough accounts");
            return Ok(None);
        }
        
        // Validate that account indices are within bounds
        if accounts[2] >= account_keys.len() || accounts[6] >= account_keys.len() {
            warn!("BUY instruction: account indices out of bounds (need indices {} and {}, but only have {} accounts)", 
                accounts[2], accounts[6], account_keys.len());
            return Ok(None);
        }

        let mint = &account_keys[accounts[2]];
        let user = &account_keys[accounts[6]];

        // We don't have exact SOL amount from instruction, but we know max
        // The actual amount will be in the event logs (which we parse separately)
        // For now, create a placeholder event
        info!("🛒 BUY detected: mint={}, user={}, token_amount={}, max_sol={}", 
            mint, user, token_amount, max_sol_cost);

        // Return None here because we'll get the accurate event from logs
        // This is just for detection/logging purposes
        Ok(None)
    }

    /// Parse SELL instruction from accounts and data
    fn parse_sell_instruction(
        &self,
        data: &[u8],
        account_keys: &[String],
        accounts: Vec<usize>,
        signature: &Signature,
        slot: u64,
        block_time: i64,
    ) -> Result<Option<PumpEvent>> {
        // SELL instruction args from IDL:
        // - amount: u64 (token amount to sell)
        // - min_sol_output: u64 (slippage protection)
        
        if data.len() < 16 {
            warn!("SELL instruction data too short");
            return Ok(None);
        }

        let token_amount = u64::from_le_bytes(data[0..8].try_into()?);
        let min_sol_output = u64::from_le_bytes(data[8..16].try_into()?);

        // SELL instruction accounts from IDL (in order):
        // 0: global, 1: feeRecipient, 2: mint, 3: bondingCurve,
        // 4: associatedBondingCurve, 5: associatedUser, 6: user (signer)
        
        if accounts.len() < 7 {
            warn!("SELL instruction: not enough accounts");
            return Ok(None);
        }
        
        // Validate that account indices are within bounds
        if accounts[2] >= account_keys.len() || accounts[6] >= account_keys.len() {
            warn!("SELL instruction: account indices out of bounds (need indices {} and {}, but only have {} accounts)", 
                accounts[2], accounts[6], account_keys.len());
            return Ok(None);
        }

        let mint = &account_keys[accounts[2]];
        let user = &account_keys[accounts[6]];

        info!("💰 SELL detected: mint={}, user={}, token_amount={}, min_sol={}", 
            mint, user, token_amount, min_sol_output);

        // Return None - we'll get accurate data from event logs
        Ok(None)
    }

    /// Parse CREATE instruction
    fn parse_create_instruction(
        &self,
        data: &[u8],
        account_keys: &[String],
        accounts: Vec<usize>,
        signature: &Signature,
        slot: u64,
        block_time: i64,
    ) -> Result<Option<PumpEvent>> {
        // CREATE instruction args from IDL:
        // - name: String
        // - symbol: String
        // - uri: String
        
        let mut offset = 0;
        let name = self.read_borsh_string(data, &mut offset)?;
        let symbol = self.read_borsh_string(data, &mut offset)?;
        let uri = self.read_borsh_string(data, &mut offset)?;

        // CREATE instruction accounts:
        // 0: mint (signer), 1: mintAuthority, 2: bondingCurve, 3: associatedBondingCurve,
        // 7: user (signer/creator)
        
        if accounts.len() < 8 {
            warn!("CREATE instruction: not enough accounts");
            return Ok(None);
        }
        
        // Validate that account indices are within bounds
        if accounts[0] >= account_keys.len() || accounts[2] >= account_keys.len() || accounts[7] >= account_keys.len() {
            warn!("CREATE instruction: account indices out of bounds (need indices {}, {}, and {}, but only have {} accounts)", 
                accounts[0], accounts[2], accounts[7], account_keys.len());
            return Ok(None);
        }

        let mint = &account_keys[accounts[0]];
        let bonding_curve = &account_keys[accounts[2]];
        let creator = &account_keys[accounts[7]];

        info!("✨ CREATE detected: mint={}, name={}, symbol={}, creator={}", 
            mint, name, symbol, creator);

        // Return None - we'll get this from event logs with full metadata
        Ok(None)
    }

    fn extract_event_data(&self, log: &str) -> Option<Vec<u8>> {
        // Anchor logs events as "Program data: <base64>"
        if let Some(start) = log.find("Program data: ") {
            let data_str = &log[start + 14..].trim();
            // Try base64 first (Anchor default)
            if let Ok(decoded) = general_purpose::STANDARD.decode(data_str) {
                return Some(decoded);
            }
            // Fallback to base58
            if let Ok(decoded) = bs58::decode(data_str).into_vec() {
                return Some(decoded);
            }
        }
        None
    }

    fn parse_event(
        &self,
        event_data: &[u8],
        signature: &Signature,
        slot: u64,
        block_time: i64,
    ) -> Result<Option<PumpEvent>> {
        if event_data.len() < 8 {
            debug!("Event data too short: {} bytes", event_data.len());
            return Ok(None);
        }

        let discriminator = &event_data[0..8];
        debug!("Event discriminator: {:?}", discriminator);
        let data = &event_data[8..];

        match discriminator {
            disc if disc == CREATE_EVENT_DISCRIMINATOR => {
                debug!("✨ Parsing CREATE event");
                self.parse_create_event_data(data, signature, slot, block_time)
            }
            disc if disc == TRADE_EVENT_DISCRIMINATOR => {
                debug!("✨ Parsing TRADE event");
                self.parse_trade_event_data(data, signature, slot, block_time)
            }
            disc if disc == COMPLETE_PUMP_AMM_MIGRATION_EVENT_DISCRIMINATOR => {
                debug!("✨ Parsing MIGRATION event");
                self.parse_migrate_event_data(data, signature, slot, block_time)
            }
            _ => {
                debug!("Unknown event discriminator: {:?}", discriminator);
                Ok(None)
            }
        }
    }

    fn parse_create_event_data(
        &self,
        data: &[u8],
        signature: &Signature,
        slot: u64,
        block_time: i64,
    ) -> Result<Option<PumpEvent>> {
        let mut offset = 0;

        let name = self.read_borsh_string(data, &mut offset)?;
        let symbol = self.read_borsh_string(data, &mut offset)?;
        let uri = self.read_borsh_string(data, &mut offset)?;

        let mint = self.read_pubkey(data, &mut offset)?;
        let bonding_curve = self.read_pubkey(data, &mut offset)?;
        let user = self.read_pubkey(data, &mut offset)?;

        debug!(
            "✨ Parsed CREATE event: mint={}, name={}, symbol={}, creator={}",
            mint, name, symbol, user
        );

        Ok(Some(PumpEvent::Launch {
            mint: mint.to_string(),
            creator: user.to_string(),
            bonding_curve: bonding_curve.to_string(),
            name,
            symbol,
            uri,
            slot,
            block_time,
            signature: signature.to_string(),
        }))
    }

    fn parse_trade_event_data(
        &self,
        data: &[u8],
        signature: &Signature,
        slot: u64,
        block_time: i64,
    ) -> Result<Option<PumpEvent>> {
        let mut offset = 0;

        let mint = self.read_pubkey(data, &mut offset)?;
        let sol_amount = self.read_u64(data, &mut offset)?;
        let token_amount = self.read_u64(data, &mut offset)?;
        let is_buy = self.read_bool(data, &mut offset)?;
        let user = self.read_pubkey(data, &mut offset)?;
        let _timestamp = self.read_i64(data, &mut offset)?;
        let _virtual_sol_reserves = self.read_u64(data, &mut offset)?;
        let _virtual_token_reserves = self.read_u64(data, &mut offset)?;

        let price = if token_amount > 0 {
            ((sol_amount as f64) / 1e9) / (token_amount as f64)
        } else {
            0.0
        };

        debug!(
            "Parsed trade event: mint={}, side={:?}, amount_sol={}, price={}",
            mint, 
            if is_buy { TradeSide::Buy } else { TradeSide::Sell }, 
            (sol_amount as f64) / 1e9, 
            price
        );

        Ok(Some(PumpEvent::Trade {
            signature: signature.to_string(),
            slot,
            block_time,
            mint: mint.to_string(),
            side: if is_buy {
                TradeSide::Buy
            } else {
                TradeSide::Sell
            },
            trader: user.to_string(),
            amount_tokens: token_amount,
            amount_sol: sol_amount,
            price,
            is_amm: false,
        }))
    }

    fn parse_migrate_event_data(
        &self,
        data: &[u8],
        signature: &Signature,
        slot: u64,
        block_time: i64,
    ) -> Result<Option<PumpEvent>> {
        let mut offset = 0;

        let _user = self.read_pubkey(data, &mut offset)?;
        let mint = self.read_pubkey(data, &mut offset)?;
        let bonding_curve = self.read_pubkey(data, &mut offset)?;
        let _timestamp = self.read_i64(data, &mut offset)?;

        debug!(
            "✨ Parsed COMPLETE (migration) event: mint={}, bonding_curve={}, sig={}",
            mint, bonding_curve, signature
        );

        Ok(Some(PumpEvent::Migrated {
            mint: mint.to_string(),
            pool: bonding_curve.to_string(),
            slot,
            block_time,
            signature: signature.to_string(),
        }))
    }

    // Helper functions for Borsh deserialization

    fn read_borsh_string(&self, data: &[u8], offset: &mut usize) -> Result<String> {
        if *offset + 4 > data.len() {
            anyhow::bail!("Not enough data to read string length");
        }
        let len = u32::from_le_bytes([
            data[*offset],
            data[*offset + 1],
            data[*offset + 2],
            data[*offset + 3],
        ]) as usize;
        *offset += 4;

        if *offset + len > data.len() {
            anyhow::bail!("Not enough data to read string content");
        }
        let s = String::from_utf8(data[*offset..*offset + len].to_vec())?;
        *offset += len;
        Ok(s)
    }

    fn read_pubkey(&self, data: &[u8], offset: &mut usize) -> Result<Pubkey> {
        if *offset + 32 > data.len() {
            anyhow::bail!("Not enough data to read pubkey");
        }
        let pubkey = Pubkey::try_from(&data[*offset..*offset + 32])?;
        *offset += 32;
        Ok(pubkey)
    }

    fn read_u64(&self, data: &[u8], offset: &mut usize) -> Result<u64> {
        if *offset + 8 > data.len() {
            anyhow::bail!("Not enough data to read u64");
        }
        let value = u64::from_le_bytes([
            data[*offset],
            data[*offset + 1],
            data[*offset + 2],
            data[*offset + 3],
            data[*offset + 4],
            data[*offset + 5],
            data[*offset + 6],
            data[*offset + 7],
        ]);
        *offset += 8;
        Ok(value)
    }

    fn read_i64(&self, data: &[u8], offset: &mut usize) -> Result<i64> {
        if *offset + 8 > data.len() {
            anyhow::bail!("Not enough data to read i64");
        }
        let value = i64::from_le_bytes([
            data[*offset],
            data[*offset + 1],
            data[*offset + 2],
            data[*offset + 3],
            data[*offset + 4],
            data[*offset + 5],
            data[*offset + 6],
            data[*offset + 7],
        ]);
        *offset += 8;
        Ok(value)
    }

    fn read_bool(&self, data: &[u8], offset: &mut usize) -> Result<bool> {
        if *offset + 1 > data.len() {
            anyhow::bail!("Not enough data to read bool");
        }
        let value = data[*offset] != 0;
        *offset += 1;
        Ok(value)
    }
}
pub mod raydium;
