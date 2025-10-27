use anyhow::{anyhow, Result};
use solana_sdk::{
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    system_program,
    sysvar,
};
use std::str::FromStr;
use log::debug;

// Manually define SPL Token and Associated Token Account IDs to avoid dependency issues
const SPL_TOKEN_PROGRAM_ID: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const SPL_ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";

// Pump.fun program constants
pub const PUMP_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";
pub const PUMP_GLOBAL_STATE: &str = "4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf";
pub const PUMP_FEE_RECIPIENT: &str = "CebN5WGQ4jvEPvsVU4EoHEpgzq1VV7AbicfhtW4xC9iM";
pub const PUMP_EVENT_AUTHORITY: &str = "Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1";

const BONDING_CURVE_SEED: &[u8] = b"bonding-curve";

/// Calculate buy instruction discriminator
/// Using Anchor's discriminator: first 8 bytes of sha256("global:buy")
fn buy_discriminator() -> [u8; 8] {
    [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea]
}

/// Calculate sell instruction discriminator
/// Using Anchor's discriminator: first 8 bytes of sha256("global:sell")
fn sell_discriminator() -> [u8; 8] {
    [0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad]
}

/// Derive bonding curve PDA
pub fn derive_bonding_curve_address(mint: &Pubkey) -> Result<Pubkey> {
    let program_id = Pubkey::from_str(PUMP_PROGRAM_ID)?;
    
    let (pda, _bump) = Pubkey::find_program_address(
        &[BONDING_CURVE_SEED, mint.as_ref()],
        &program_id,
    );
    
    Ok(pda)
}

/// Get associated token account address
/// Manually implements: get_associated_token_address_with_program_id
pub fn get_associated_token_address(wallet: &Pubkey, mint: &Pubkey) -> Pubkey {
    let ata_program_id = Pubkey::from_str(SPL_ASSOCIATED_TOKEN_ACCOUNT_PROGRAM_ID).unwrap();
    let token_program_id = Pubkey::from_str(SPL_TOKEN_PROGRAM_ID).unwrap();
    
    let (address, _bump) = Pubkey::find_program_address(
        &[
            wallet.as_ref(),
            token_program_id.as_ref(),
            mint.as_ref(),
        ],
        &ata_program_id,
    );
    
    address
}

/// Build Pump.fun BUY instruction with OFFICIAL IDL account structure (16 accounts)
/// 
/// # Arguments
/// * `buyer` - The buyer's public key (signer)
/// * `mint` - The token mint address
/// * `token_amount` - Amount of tokens to buy (in base units, with decimals)
/// * `max_sol_cost` - Maximum SOL to spend (slippage protection, in lamports)
/// * `bonding_curve_creator` - Creator pubkey from bonding curve state (needed for creator_vault PDA)
pub fn create_buy_instruction(
    buyer: &Pubkey,
    mint: &Pubkey,
    token_amount: u64,
    max_sol_cost: u64,
    bonding_curve_creator: &Pubkey,
) -> Result<Instruction> {
    debug!("🔨 Building Pump.fun BUY instruction (OFFICIAL 16-account structure)");
    debug!("   Token amount: {}", token_amount);
    debug!("   Max SOL cost: {} lamports", max_sol_cost);
    
    // Parse program IDs
    let program_id = Pubkey::from_str(PUMP_PROGRAM_ID)?;
    let global_state = Pubkey::from_str(PUMP_GLOBAL_STATE)?;
    let fee_recipient = Pubkey::from_str(PUMP_FEE_RECIPIENT)?;
    let event_authority = Pubkey::from_str(PUMP_EVENT_AUTHORITY)?;
    let token_program = Pubkey::from_str(SPL_TOKEN_PROGRAM_ID)?;
    
    // Derive PDAs
    let bonding_curve = derive_bonding_curve_address(mint)?;
    let associated_bonding_curve = get_associated_token_address(&bonding_curve, mint);
    let associated_user = get_associated_token_address(buyer, mint);
    
    // Derive creator_vault PDA: seeds = ["creator-vault", bonding_curve.creator]
    let (creator_vault, _) = Pubkey::find_program_address(
        &[b"creator-vault", bonding_curve_creator.as_ref()],
        &program_id,
    );
    
    // Derive global_volume_accumulator PDA: seeds = ["global_volume_accumulator"]
    let (global_volume_accumulator, _) = Pubkey::find_program_address(
        &[b"global_volume_accumulator"],
        &program_id,
    );
    
    // Derive user_volume_accumulator PDA: seeds = ["user_volume_accumulator", user]
    let (user_volume_accumulator, _) = Pubkey::find_program_address(
        &[b"user_volume_accumulator", buyer.as_ref()],
        &program_id,
    );
    
    // Fee program (optional but included)
    let fee_program = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")?;
    
    // Derive fee_config PDA: seeds = ["fee_config", <fixed_seed>]
    // Fixed seed from IDL: [1, 86, 224, 246, 147, 102, 90, 207, 68, 219, 21, 104, 191, 23, 91, 170, 81, 137, 203, 151, 245, 210, 255, 59, 101, 93, 43, 182, 253, 109, 24, 176]
    let fixed_seed: [u8; 32] = [1, 86, 224, 246, 147, 102, 90, 207, 68, 219, 21, 104, 191, 23, 91, 170, 81, 137, 203, 151, 245, 210, 255, 59, 101, 93, 43, 182, 253, 109, 24, 176];
    let (fee_config, _) = Pubkey::find_program_address(
        &[b"fee_config", &fixed_seed],
        &fee_program,
    );
    
    debug!("   Bonding curve: {}", bonding_curve);
    debug!("   User ATA: {}", associated_user);
    debug!("   Creator vault: {}", creator_vault);
    debug!("   Global volume accumulator: {}", global_volume_accumulator);
    debug!("   User volume accumulator: {}", user_volume_accumulator);
    debug!("   Fee config: {}", fee_config);
    debug!("   Fee program: {}", fee_program);
    
    // Build instruction data: [discriminator (8 bytes)] + [amount (8 bytes)] + [maxSolCost (8 bytes)]
    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(&buy_discriminator());
    data.extend_from_slice(&token_amount.to_le_bytes());
    data.extend_from_slice(&max_sol_cost.to_le_bytes());
    
    // Build accounts (OFFICIAL IDL ORDER - 16 accounts total!)
    let accounts = vec![
        AccountMeta::new_readonly(global_state, false),           // 0: global
        AccountMeta::new(fee_recipient, false),                   // 1: fee_recipient (writable)
        AccountMeta::new_readonly(*mint, false),                  // 2: mint
        AccountMeta::new(bonding_curve, false),                   // 3: bonding_curve (writable)
        AccountMeta::new(associated_bonding_curve, false),        // 4: associated_bonding_curve (writable)
        AccountMeta::new(associated_user, false),                 // 5: associated_user (writable)
        AccountMeta::new(*buyer, true),                           // 6: user (signer + writable)
        AccountMeta::new_readonly(system_program::ID, false),     // 7: system_program
        AccountMeta::new_readonly(token_program, false),          // 8: token_program
        AccountMeta::new(creator_vault, false),                   // 9: creator_vault (writable)
        AccountMeta::new_readonly(event_authority, false),        // 10: event_authority
        AccountMeta::new_readonly(program_id, false),             // 11: program
        AccountMeta::new(global_volume_accumulator, false),       // 12: global_volume_accumulator (writable)
        AccountMeta::new(user_volume_accumulator, false),         // 13: user_volume_accumulator (writable)
        AccountMeta::new_readonly(fee_config, false),             // 14: fee_config (optional)
        AccountMeta::new_readonly(fee_program, false),            // 15: fee_program (optional)
    ];
    
    Ok(Instruction {
        program_id,
        accounts,
        data,
    })
}

/// Build Pump.fun SELL instruction with OFFICIAL IDL account structure (14 accounts)
/// 
/// # Arguments
/// * `seller` - The seller's public key (signer)
/// * `mint` - The token mint address
/// * `token_amount` - Amount of tokens to sell (in base units, with decimals)
/// * `min_sol_output` - Minimum SOL to receive (slippage protection, in lamports)
/// * `bonding_curve_creator` - Creator pubkey from bonding curve state (needed for creator_vault PDA)
pub fn create_sell_instruction(
    seller: &Pubkey,
    mint: &Pubkey,
    token_amount: u64,
    min_sol_output: u64,
    bonding_curve_creator: &Pubkey,
) -> Result<Instruction> {
    debug!("🔨 Building Pump.fun SELL instruction (OFFICIAL 14-account structure)");
    debug!("   Token amount: {}", token_amount);
    debug!("   Min SOL output: {} lamports", min_sol_output);
    
    // Parse program IDs
    let program_id = Pubkey::from_str(PUMP_PROGRAM_ID)?;
    let global_state = Pubkey::from_str(PUMP_GLOBAL_STATE)?;
    let fee_recipient = Pubkey::from_str(PUMP_FEE_RECIPIENT)?;
    let event_authority = Pubkey::from_str(PUMP_EVENT_AUTHORITY)?;
    let token_program = Pubkey::from_str(SPL_TOKEN_PROGRAM_ID)?;
    
    // Derive PDAs
    let bonding_curve = derive_bonding_curve_address(mint)?;
    let associated_bonding_curve = get_associated_token_address(&bonding_curve, mint);
    let associated_user = get_associated_token_address(seller, mint);
    
    // Derive creator_vault PDA: seeds = ["creator-vault", bonding_curve.creator]
    let (creator_vault, _) = Pubkey::find_program_address(
        &[b"creator-vault", bonding_curve_creator.as_ref()],
        &program_id,
    );
    
    // Fee program (optional but included)
    let fee_program = Pubkey::from_str("pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ")?;
    
    // Derive fee_config PDA: seeds = ["fee_config", <fixed_seed>]
    let fixed_seed: [u8; 32] = [1, 86, 224, 246, 147, 102, 90, 207, 68, 219, 21, 104, 191, 23, 91, 170, 81, 137, 203, 151, 245, 210, 255, 59, 101, 93, 43, 182, 253, 109, 24, 176];
    let (fee_config, _) = Pubkey::find_program_address(
        &[b"fee_config", &fixed_seed],
        &fee_program,
    );
    
    debug!("   Bonding curve: {}", bonding_curve);
    debug!("   User ATA: {}", associated_user);
    debug!("   Creator vault: {}", creator_vault);
    debug!("   Fee config: {}", fee_config);
    debug!("   Fee program: {}", fee_program);
    
    // Build instruction data: [discriminator (8 bytes)] + [amount (8 bytes)] + [minSolOutput (8 bytes)]
    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(&sell_discriminator());
    data.extend_from_slice(&token_amount.to_le_bytes());
    data.extend_from_slice(&min_sol_output.to_le_bytes());
    
    // Build accounts (OFFICIAL IDL ORDER - 14 accounts total!)
    let accounts = vec![
        AccountMeta::new_readonly(global_state, false),           // 0: global
        AccountMeta::new(fee_recipient, false),                   // 1: fee_recipient (writable)
        AccountMeta::new_readonly(*mint, false),                  // 2: mint
        AccountMeta::new(bonding_curve, false),                   // 3: bonding_curve (writable)
        AccountMeta::new(associated_bonding_curve, false),        // 4: associated_bonding_curve (writable)
        AccountMeta::new(associated_user, false),                 // 5: associated_user (writable)
        AccountMeta::new(*seller, true),                          // 6: user (signer + writable)
        AccountMeta::new_readonly(system_program::ID, false),     // 7: system_program
        AccountMeta::new(creator_vault, false),                   // 8: creator_vault (writable)
        AccountMeta::new_readonly(token_program, false),          // 9: token_program
        AccountMeta::new_readonly(event_authority, false),        // 10: event_authority
        AccountMeta::new_readonly(program_id, false),             // 11: program
        AccountMeta::new_readonly(fee_config, false),             // 12: fee_config (optional)
        AccountMeta::new_readonly(fee_program, false),            // 13: fee_program (optional)
    ];
    
    Ok(Instruction {
        program_id,
        accounts,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_discriminators() {
        // Verify our discriminators match expected values
        assert_eq!(buy_discriminator(), [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea]);
        assert_eq!(sell_discriminator(), [0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad]);
    }
    
    #[test]
    fn test_bonding_curve_derivation() {
        let mint = Pubkey::from_str("GBX4a3zACNfp8MdTvYMoLwjf5gKUmZxHyYQgaeJqXkMD").unwrap();
        let curve = derive_bonding_curve_address(&mint).unwrap();
        
        // Should derive consistently
        let curve2 = derive_bonding_curve_address(&mint).unwrap();
        assert_eq!(curve, curve2);
    }
    
    #[test]
    fn test_buy_instruction_creation() {
        let buyer = Pubkey::from_str("11111111111111111111111111111111").unwrap();
        let mint = Pubkey::from_str("GBX4a3zACNfp8MdTvYMoLwjf5gKUmZxHyYQgaeJqXkMD").unwrap();
        let creator = Pubkey::default();  // Dummy creator for test
        
        let ix = create_buy_instruction(
            &buyer,
            &mint,
            1_000_000, // 1 token (6 decimals)
            10_000_000, // 0.01 SOL
            &creator,  // NEW - creator pubkey
        ).unwrap();
        
        // Should have correct program ID
        assert_eq!(ix.program_id.to_string(), PUMP_PROGRAM_ID);
        
        // Should have 16 accounts (OFFICIAL IDL STRUCTURE!)
        assert_eq!(ix.accounts.len(), 16);
        
        // Data should be 24 bytes (8 discriminator + 8 amount + 8 maxSolCost)
        assert_eq!(ix.data.len(), 24);
    }
}
