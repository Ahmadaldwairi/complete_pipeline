// Executor Configuration - Lightweight Execution Only
// All strategy parameters moved to Brain service

use std::env;

#[derive(Clone)]
pub struct Config {
    // ============================================================================
    // GRPC & RPC CONNECTIVITY
    // ============================================================================
    pub grpc_endpoint: String,
    pub rpc_endpoint: String,
    pub websocket_endpoint: String,
    
    // ============================================================================
    // WALLET & KEYPAIR
    // ============================================================================
    pub wallet_private_key: String,
    
    // ============================================================================
    // EXECUTION MODE
    // ============================================================================
    pub use_tpu: bool,
    pub use_jito: bool,
    
    // ============================================================================
    // JITO CONFIGURATION
    // ============================================================================
    pub jito_block_engine_url: String,
    pub jito_tip_account: String,
    pub jito_tip_amount: u64,
    pub jito_use_dynamic_tip: bool,
    pub jito_entry_percentile: f64,
    pub jito_exit_percentile: f64,
    
    // ============================================================================
    // TELEGRAM NOTIFICATIONS (async, non-blocking)
    // ============================================================================
    pub telegram_bot_token: String,
    pub telegram_chat_id: String,
    pub telegram_async_queue: usize,
    
    // ============================================================================
    // ADVICE BUS (receives TradeDecisions from Brain)
    // ============================================================================
    pub advisor_enabled: bool,
    pub advisor_queue_size: usize,
    pub advice_only_mode: bool,
    
    // Optional: Advice constraints (apply only to advisory overrides)
    pub advice_min_confidence: u8,
    pub advice_max_hold_extension_secs: u64,
    pub advice_max_exit_slippage_bps: u16,
    
    // ============================================================================
    // BRAIN TELEMETRY (send execution results back to Brain)
    // ============================================================================
    pub brain_telemetry_enabled: bool,
    pub brain_telemetry_host: String,
    pub brain_telemetry_port: u16,
    
    // ============================================================================
    // DATABASE (log executed trades & realized PnL)
    // ============================================================================
    pub db_host: String,
    pub db_port: u16,
    pub db_name: String,
    pub db_user: String,
    pub db_password: String,
    
    // ============================================================================
    // EXECUTION LIMITS (safety only, not strategy)
    // ============================================================================
    pub max_builder_threads: usize,
    pub network_timeout_ms: u64,
    pub retry_on_fail: bool,
    pub max_retries: u32,
    pub price_check_interval: u64,
}

impl Config {
    pub fn from_env() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Check required vars first
        let _telegram_token = env::var("TELEGRAM_BOT_TOKEN")
            .map_err(|_| "Missing TELEGRAM_BOT_TOKEN in .env")?;
        let _telegram_chat = env::var("TELEGRAM_CHAT_ID")
            .map_err(|_| "Missing TELEGRAM_CHAT_ID in .env")?;
        let _wallet_key = env::var("WALLET_PRIVATE_KEY")
            .map_err(|_| "Missing WALLET_PRIVATE_KEY in .env")?;
        let _db_host = env::var("DB_HOST")
            .map_err(|_| "Missing DB_HOST in .env")?;
        let _db_port = env::var("DB_PORT")
            .map_err(|_| "Missing DB_PORT in .env")?;
        let _db_name = env::var("DB_NAME")
            .map_err(|_| "Missing DB_NAME in .env")?;
        let _db_user = env::var("DB_USER")
            .map_err(|_| "Missing DB_USER in .env")?;
        let _db_password = env::var("DB_PASSWORD")
            .map_err(|_| "Missing DB_PASSWORD in .env")?;
        
        Ok(Config {
            // GRPC & RPC
            grpc_endpoint: env::var("GRPC_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:10000".to_string()),
            rpc_endpoint: env::var("RPC_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:8899".to_string()),
            websocket_endpoint: env::var("WEBSOCKET_ENDPOINT")
                .unwrap_or_else(|_| "ws://127.0.0.1:8900".to_string()),
            
            // Wallet
            wallet_private_key: env::var("WALLET_PRIVATE_KEY")?,
            
            // Execution Mode
            use_tpu: env::var("USE_TPU")
                .unwrap_or_else(|_| "true".to_string())
                .parse()?,
            use_jito: env::var("USE_JITO")
                .unwrap_or_else(|_| "false".to_string())
                .parse()?,
            
            // Jito
            jito_block_engine_url: env::var("JITO_BLOCK_ENGINE_URL")
                .unwrap_or_else(|_| "https://mainnet.block-engine.jito.wtf".to_string()),
            jito_tip_account: env::var("JITO_TIP_ACCOUNT")
                .unwrap_or_else(|_| "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5".to_string()),
            jito_tip_amount: env::var("JITO_TIP_AMOUNT")
                .unwrap_or_else(|_| "10000".to_string())
                .parse()?,
            jito_use_dynamic_tip: env::var("JITO_USE_DYNAMIC_TIP")
                .unwrap_or_else(|_| "false".to_string())
                .parse()?,
            jito_entry_percentile: env::var("JITO_ENTRY_PERCENTILE")
                .unwrap_or_else(|_| "95.0".to_string())
                .parse()?,
            jito_exit_percentile: env::var("JITO_EXIT_PERCENTILE")
                .unwrap_or_else(|_| "50.0".to_string())
                .parse()?,
            
            // Telegram
            telegram_bot_token: env::var("TELEGRAM_BOT_TOKEN")?,
            telegram_chat_id: env::var("TELEGRAM_CHAT_ID")?,
            telegram_async_queue: env::var("TELEGRAM_ASYNC_QUEUE")
                .unwrap_or_else(|_| "100".to_string())
                .parse()?,
            
            // Advice Bus
            advisor_enabled: env::var("ADVISOR_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()?,
            advisor_queue_size: env::var("ADVISOR_QUEUE_SIZE")
                .unwrap_or_else(|_| "5".to_string())
                .parse()?,
            advice_only_mode: env::var("ADVICE_ONLY_MODE")
                .unwrap_or_else(|_| "false".to_string())
                .parse()?,
            advice_min_confidence: env::var("ADVICE_MIN_CONFIDENCE")
                .unwrap_or_else(|_| "70".to_string())
                .parse()?,
            advice_max_hold_extension_secs: env::var("ADVICE_MAX_HOLD_EXTENSION_SECS")
                .unwrap_or_else(|_| "30".to_string())
                .parse()?,
            advice_max_exit_slippage_bps: env::var("ADVICE_MAX_EXIT_SLIPPAGE_BPS")
                .unwrap_or_else(|_| "500".to_string())
                .parse()?,
            
            // Brain Telemetry
            brain_telemetry_enabled: env::var("BRAIN_TELEMETRY_ENABLED")
                .unwrap_or_else(|_| "true".to_string())
                .parse()?,
            brain_telemetry_host: env::var("BRAIN_TELEMETRY_HOST")
                .unwrap_or_else(|_| "127.0.0.1".to_string()),
            brain_telemetry_port: env::var("BRAIN_TELEMETRY_PORT")
                .unwrap_or_else(|_| "45110".to_string())
                .parse()?,
            
            // Database
            db_host: env::var("DB_HOST")?,
            db_port: env::var("DB_PORT")?.parse()?,
            db_name: env::var("DB_NAME")?,
            db_user: env::var("DB_USER")?,
            db_password: env::var("DB_PASSWORD")?,
            
            // Execution Limits
            max_builder_threads: env::var("MAX_BUILDER_THREADS")
                .unwrap_or_else(|_| "4".to_string())
                .parse()?,
            network_timeout_ms: env::var("NETWORK_TIMEOUT_MS")
                .unwrap_or_else(|_| "5000".to_string())
                .parse()?,
            retry_on_fail: env::var("RETRY_ON_FAIL")
                .unwrap_or_else(|_| "true".to_string())
                .parse()?,
            max_retries: env::var("MAX_RETRIES")
                .unwrap_or_else(|_| "3".to_string())
                .parse()?,
            price_check_interval: env::var("PRICE_CHECK_INTERVAL")
                .unwrap_or_else(|_| "200".to_string())
                .parse()?,
        })
    }
}
