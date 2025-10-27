//! Pump.fun instruction decoder

use anyhow::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Create,
    Buy,
    Sell,
    Withdraw,
    Initialize,  // Admin: creates global state
    SetParams,   // Admin: sets global parameters
    Unknown,
}

impl Action {
    pub fn as_str(&self) -> &'static str {
        match self {
            Action::Create => "CREATE",
            Action::Buy => "BUY",
            Action::Sell => "SELL",
            Action::Withdraw => "WITHDRAW",
            Action::Initialize => "INITIALIZE",
            Action::SetParams => "SET_PARAMS",
            Action::Unknown => "UNKNOWN",
        }
    }
}

#[derive(Debug, Clone)]
pub struct DecodedInstruction {
    pub action: Action,
    pub mint: Option<String>,
    pub user_wallet: Option<String>, // The wallet performing the action
    pub token_amount: Option<u64>,
    pub max_sol_cost: Option<u64>, // For BUY: max SOL to spend; For SELL: min SOL to receive
    pub decode_ok: bool,
    pub decode_err: Option<String>,
}

/// Pump.fun instruction discriminators (first 8 bytes of instruction data)
/// These are derived from the method name hashes in the Pump.fun program
/// Format: SHA256("global:<method_name>")[0..8]
const DISCRIMINATOR_INITIALIZE: [u8; 8] = [0xaf, 0xaf, 0x6d, 0x1f, 0x0d, 0x98, 0x9b, 0xed];
const DISCRIMINATOR_SET_PARAMS: [u8; 8] = [0xa5, 0x1f, 0x86, 0x35, 0xbd, 0xb4, 0x82, 0xff];
const DISCRIMINATOR_CREATE: [u8; 8] = [0x18, 0x1e, 0xc8, 0x28, 0x05, 0x1c, 0x07, 0x77];
const DISCRIMINATOR_BUY: [u8; 8] = [0x66, 0x06, 0x3d, 0x12, 0x01, 0xda, 0xeb, 0xea];
const DISCRIMINATOR_SELL: [u8; 8] = [0x33, 0xe6, 0x85, 0xa4, 0x01, 0x7f, 0x83, 0xad];
const DISCRIMINATOR_WITHDRAW: [u8; 8] = [0xb7, 0x12, 0x46, 0x9c, 0x94, 0x6d, 0xa1, 0x22];

/// Decode a Pump.fun instruction by discriminator
pub fn decode_instruction(data: &[u8], accounts: &[String]) -> Result<DecodedInstruction> {
    if data.len() < 8 {
        let err_msg = format!("Instruction data too short: {} bytes (expected at least 8)", data.len());
        return Ok(DecodedInstruction {
            action: Action::Unknown,
            mint: None,
            user_wallet: None,
            token_amount: None,
            max_sol_cost: None,
            decode_ok: false,
            decode_err: Some(err_msg),
        });
    }

    let discriminator = &data[0..8];
    
    let action = if discriminator == DISCRIMINATOR_CREATE {
        Action::Create
    } else if discriminator == DISCRIMINATOR_BUY {
        Action::Buy
    } else if discriminator == DISCRIMINATOR_SELL {
        Action::Sell
    } else if discriminator == DISCRIMINATOR_WITHDRAW {
        Action::Withdraw
    } else if discriminator == DISCRIMINATOR_INITIALIZE {
        // Admin operation - but categorize it properly
        Action::Initialize
    } else if discriminator == DISCRIMINATOR_SET_PARAMS {
        // Admin operation - but categorize it properly
        Action::SetParams
    } else {
        // Log unknown discriminator for analysis
        let disc_hex = discriminator.iter()
            .map(|b| format!("{:02x}", b))
            .collect::<Vec<_>>()
            .join(" ");
        tracing::warn!("⚠️  Unknown Pump.fun discriminator: [{}]", disc_hex);
        Action::Unknown
    };

    // Extract mint from accounts based on instruction type
    // Based on official Pump.fun IDL:
    // - CREATE: mint is at index 0
    // - BUY: mint is at index 2, user is at index 3
    // - SELL: mint is at index 2, user is at index 3
    // - WITHDRAW: mint is at index 2, user is at index 3
    // - INITIALIZE: no mint (global state creation)
    // - SET_PARAMS: no mint (global parameter update)
    let mint = match action {
        Action::Create => accounts.get(0).cloned(),
        Action::Buy | Action::Sell | Action::Withdraw => accounts.get(2).cloned(),
        Action::Initialize | Action::SetParams | Action::Unknown => None,
    };

    // Extract user wallet from accounts
    // For BUY/SELL/WITHDRAW: user wallet is at index 3
    // For CREATE: user wallet is at index 1 (creator)
    let user_wallet = match action {
        Action::Create => accounts.get(1).cloned(),
        Action::Buy | Action::Sell | Action::Withdraw => accounts.get(3).cloned(),
        Action::Initialize | Action::SetParams | Action::Unknown => None,
    };

    // Parse instruction data:
    // For BUY/SELL: [discriminator (8)] + [token_amount (8)] + [max_sol_cost (8)]
    let token_amount = if data.len() >= 16 {
        Some(u64::from_le_bytes(
            data[8..16].try_into().unwrap_or([0u8; 8])
        ))
    } else {
        None
    };

    let max_sol_cost = if data.len() >= 24 {
        Some(u64::from_le_bytes(
            data[16..24].try_into().unwrap_or([0u8; 8])
        ))
    } else {
        None
    };

    // Determine if decode was successful
    let (decode_ok, decode_err) = match action {
        Action::Unknown => {
            let disc_hex = discriminator.iter()
                .map(|b| format!("{:02x}", b))
                .collect::<Vec<_>>()
                .join(" ");
            (false, Some(format!("Unknown discriminator: [{}]", disc_hex)))
        },
        _ => (true, None),
    };

    Ok(DecodedInstruction {
        action,
        mint,
        user_wallet,
        token_amount,
        max_sol_cost,
        decode_ok,
        decode_err,
    })
}
