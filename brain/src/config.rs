//! Configuration management for the Brain service
//! 
//! Loads configuration from environment variables (via .env file) and provides
//! validated, type-safe access to all service parameters.

use anyhow::{Context, Result};
use std::env;
use std::net::IpAddr;
use std::path::PathBuf;
use std::str::FromStr;

/// Complete configuration for the Brain decision engine service
#[derive(Debug, Clone)]
pub struct Config {
    pub decision: DecisionConfig,
    pub validation: ValidationConfig,
    pub guardrails: GuardrailsConfig,
    pub database: DatabaseConfig,
    pub network: NetworkConfig,
    pub logging: LoggingConfig,
    pub cache: CacheConfig,
    pub performance: PerformanceConfig,
    pub confirmation: ConfirmationConfig,
}

/// Confirmation tracking configuration
#[derive(Debug, Clone)]
pub struct ConfirmationConfig {
    /// Normal provisional position timeout (milliseconds)
    pub pending_ttl_ms: u64,
    /// Fast confirmation timeout for low mempool competition (milliseconds)
    pub fast_confirm_ttl_ms: u64,
    /// Position monitoring interval (seconds)
    pub monitoring_interval_sec: u64,
    /// Mint reservation TTL for BUY decisions (seconds)
    pub reserve_buy_ttl_sec: u64,
    /// Mint reservation TTL for SELL decisions (seconds)
    pub reserve_sell_ttl_sec: u64,
    /// BUY confirmation timeout (seconds)
    pub confirm_timeout_buy_sec: u64,
    /// SELL confirmation timeout (seconds)
    pub confirm_timeout_sell_sec: u64,
    /// Reconciliation watchdog interval (seconds)
    pub reconciliation_interval_sec: u64,
    /// Stale state threshold for reconciliation (seconds)
    pub stale_state_threshold_sec: u64,
}

/// Decision engine threshold configuration
#[derive(Debug, Clone)]
pub struct DecisionConfig {
    /// Minimum confidence score (0-100) for non-copytrade decisions
    pub min_decision_conf: u8,
    /// Minimum confidence score (0-100) for copytrade decisions
    pub min_copytrade_confidence: u8,
    /// Minimum follow-through score (0-100) to proceed
    pub min_follow_through_score: u8,
}

/// Pre-trade validation parameters
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Multiplier for fee estimation (e.g., 2.2)
    pub fee_multiplier: f64,
    /// Maximum impact as fraction of TP target (e.g., 0.45 = 45%)
    pub impact_cap_multiplier: f64,
    /// Minimum liquidity required in USD
    pub min_liquidity_usd: f64,
    /// Maximum slippage tolerance as fraction (e.g., 0.15 = 15%)
    pub max_slippage: f64,
}

/// Anti-churn guardrail configuration
#[derive(Debug, Clone)]
pub struct GuardrailsConfig {
    /// Maximum concurrent positions allowed
    pub max_concurrent_positions: usize,
    /// Maximum positions from advisor/copytrade source
    pub max_advisor_positions: usize,
    /// Rate limit for general decisions (milliseconds)
    pub rate_limit_ms: u64,
    /// Rate limit for advisor decisions (milliseconds)
    pub advisor_rate_limit_ms: u64,
    /// Loss backoff: consecutive losses before pause
    pub loss_backoff_threshold: usize,
    /// Loss backoff: time window to track losses (seconds)
    pub loss_backoff_window_secs: u64,
    /// Loss backoff: pause duration after threshold (seconds)
    pub loss_backoff_pause_secs: u64,
    /// Wallet cooling: min time between copytrading same wallet (seconds)
    pub wallet_cooling_secs: u64,
}

/// Database connection configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// PostgreSQL host for WalletTracker
    pub postgres_host: String,
    /// PostgreSQL port
    pub postgres_port: u16,
    /// PostgreSQL username
    pub postgres_user: String,
    /// PostgreSQL password
    pub postgres_password: String,
    /// PostgreSQL database name
    pub postgres_db: String,
    /// SQLite path for LaunchTracker
    pub sqlite_path: PathBuf,
}

impl DatabaseConfig {
    /// Get PostgreSQL connection string
    pub fn postgres_connection_string(&self) -> String {
        format!(
            "host={} port={} user={} password={} dbname={}",
            self.postgres_host,
            self.postgres_port,
            self.postgres_user,
            self.postgres_password,
            self.postgres_db
        )
    }
}

/// Network communication configuration
#[derive(Debug, Clone)]
pub struct NetworkConfig {
    /// Port to receive advice messages on
    pub advice_bus_port: u16,
    /// Port to send decisions to
    pub decision_bus_port: u16,
    /// UDP bind address (typically 127.0.0.1 for localhost)
    pub udp_bind_address: IpAddr,
    /// UDP receive buffer size
    pub udp_recv_buffer_size: usize,
    /// UDP send buffer size
    pub udp_send_buffer_size: usize,
    /// Yellowstone gRPC endpoint
    pub yellowstone_endpoint: String,
    /// Yellowstone x-token (optional)
    pub yellowstone_token: Option<String>,
    /// Solana RPC endpoint for polling backup
    pub rpc_url: String,
    /// Trading wallet pubkey for gRPC monitoring
    pub wallet_pubkey: String,
    /// Telegram bot token
    pub telegram_bot_token: String,
    /// Telegram chat ID
    pub telegram_chat_id: String,
}

/// Logging configuration
#[derive(Debug, Clone)]
pub struct LoggingConfig {
    /// Path to decision log CSV file
    pub decision_log_path: PathBuf,
    /// Log level (error, warn, info, debug, trace)
    pub log_level: String,
}

/// Feature cache configuration
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Mint cache capacity (number of tokens)
    pub mint_cache_capacity: usize,
    /// Wallet cache capacity (number of wallets)
    pub wallet_cache_capacity: usize,
    /// Cache refresh interval (seconds)
    pub cache_refresh_interval_secs: u64,
}

/// Performance tuning configuration
#[derive(Debug, Clone)]
pub struct PerformanceConfig {
    /// Number of worker threads (0 = auto-detect)
    pub worker_threads: usize,
}

impl Config {
    /// Load configuration from environment variables
    /// 
    /// Expects a .env file in the working directory or environment variables to be set.
    /// Returns an error if required variables are missing or invalid.
    pub fn from_env() -> Result<Self> {
        // Load .env file if it exists (ignoring error if not found)
        let _ = dotenv::dotenv();

        Ok(Config {
            decision: DecisionConfig {
                min_decision_conf: get_env_u8("MIN_DECISION_CONF", 75)?,
                min_copytrade_confidence: get_env_u8("MIN_COPYTRADE_CONFIDENCE", 70)?,
                min_follow_through_score: get_env_u8("MIN_FOLLOW_THROUGH_SCORE", 55)?,
            },
            validation: ValidationConfig {
                fee_multiplier: get_env_f64("FEE_MULTIPLIER", 2.2)?,
                impact_cap_multiplier: get_env_f64("IMPACT_CAP_MULTIPLIER", 0.45)?,
                min_liquidity_usd: get_env_f64("MIN_LIQUIDITY_USD", 5000.0)?,
                max_slippage: get_env_f64("MAX_SLIPPAGE", 0.15)?,
            },
            guardrails: GuardrailsConfig {
                max_concurrent_positions: get_env_usize("MAX_CONCURRENT_POSITIONS", 5)?,  // Increased from 3 for 1M+ MC hunting
                max_advisor_positions: get_env_usize("MAX_ADVISOR_POSITIONS", 3)?,        // Increased from 2
                rate_limit_ms: get_env_u64("RATE_LIMIT_MS", 100)?,
                advisor_rate_limit_ms: get_env_u64("ADVISOR_RATE_LIMIT_MS", 30000)?,
                loss_backoff_threshold: get_env_usize("LOSS_BACKOFF_THRESHOLD", 4)?,      // Increased from 3 ($100 positions)
                loss_backoff_window_secs: get_env_u64("LOSS_BACKOFF_WINDOW_SECS", 180)?,
                loss_backoff_pause_secs: get_env_u64("LOSS_BACKOFF_PAUSE_SECS", 120)?,
                wallet_cooling_secs: get_env_u64("WALLET_COOLING_SECS", 60)?,             // Reduced from 90 for faster reuse
            },
            database: DatabaseConfig {
                postgres_host: get_env_string("POSTGRES_HOST", "localhost")?,
                postgres_port: get_env_u16("POSTGRES_PORT", 5432)?,
                postgres_user: get_env_string("POSTGRES_USER", "trader")?,
                postgres_password: get_env_string("POSTGRES_PASSWORD", "")?,
                postgres_db: get_env_string("POSTGRES_DB", "wallet_tracker")?,
                sqlite_path: PathBuf::from(get_env_string("SQLITE_PATH", "./data/launch_tracker.db")?),
            },
            network: NetworkConfig {
                advice_bus_port: get_env_u16("ADVICE_BUS_PORT", 45100)?,
                decision_bus_port: get_env_u16("DECISION_BUS_PORT", 45110)?,
                udp_bind_address: IpAddr::from_str(&get_env_string("UDP_BIND_ADDRESS", "127.0.0.1")?)
                    .context("Invalid UDP_BIND_ADDRESS")?,
                udp_recv_buffer_size: get_env_usize("UDP_RECV_BUFFER_SIZE", 8192)?,
                udp_send_buffer_size: get_env_usize("UDP_SEND_BUFFER_SIZE", 8192)?,
                yellowstone_endpoint: get_env_string("YELLOWSTONE_ENDPOINT", "http://127.0.0.1:10000")?,
                yellowstone_token: env::var("YELLOWSTONE_TOKEN").ok(),
                rpc_url: get_env_string("RPC_URL", "https://api.mainnet-beta.solana.com")?,
                wallet_pubkey: get_env_string("WALLET_PUBKEY", "")?,
                telegram_bot_token: get_env_string("TELEGRAM_BOT_TOKEN", "")?,
                telegram_chat_id: get_env_string("TELEGRAM_CHAT_ID", "")?,
            },
            logging: LoggingConfig {
                decision_log_path: PathBuf::from(get_env_string("DECISION_LOG_PATH", "./data/brain_decisions.csv")?),
                log_level: get_env_string("LOG_LEVEL", "info")?,
            },
            cache: CacheConfig {
                mint_cache_capacity: get_env_usize("MINT_CACHE_CAPACITY", 10000)?,
                wallet_cache_capacity: get_env_usize("WALLET_CACHE_CAPACITY", 5000)?,
                cache_refresh_interval_secs: get_env_u64("CACHE_REFRESH_INTERVAL_SECS", 30)?,
            },
            performance: PerformanceConfig {
                worker_threads: get_env_usize("WORKER_THREADS", 0)?,
            },
            confirmation: ConfirmationConfig {
                pending_ttl_ms: get_env_u64("PENDING_TTL_MS", 1200)?,
                fast_confirm_ttl_ms: get_env_u64("FAST_CONFIRM_TTL_MS", 600)?,
                monitoring_interval_sec: get_env_u64("MONITORING_INTERVAL_SEC", 2)?,
                reserve_buy_ttl_sec: get_env_u64("RESERVE_BUY_TTL_SEC", 30)?,
                reserve_sell_ttl_sec: get_env_u64("RESERVE_SELL_TTL_SEC", 30)?,
                confirm_timeout_buy_sec: get_env_u64("CONFIRM_TIMEOUT_BUY_SEC", 10)?,
                confirm_timeout_sell_sec: get_env_u64("CONFIRM_TIMEOUT_SELL_SEC", 15)?,
                reconciliation_interval_sec: get_env_u64("RECONCILIATION_INTERVAL_SEC", 30)?,
                stale_state_threshold_sec: get_env_u64("STALE_STATE_THRESHOLD_SEC", 60)?,
            },
        })
    }

    /// Validate configuration values are within acceptable ranges
    pub fn validate(&self) -> Result<()> {
        // Decision thresholds
        if self.decision.min_decision_conf > 100 {
            anyhow::bail!("MIN_DECISION_CONF must be ≤ 100");
        }
        if self.decision.min_copytrade_confidence > 100 {
            anyhow::bail!("MIN_COPYTRADE_CONFIDENCE must be ≤ 100");
        }
        if self.decision.min_follow_through_score > 100 {
            anyhow::bail!("MIN_FOLLOW_THROUGH_SCORE must be ≤ 100");
        }

        // Validation parameters
        if self.validation.fee_multiplier <= 0.0 {
            anyhow::bail!("FEE_MULTIPLIER must be > 0");
        }
        if self.validation.impact_cap_multiplier < 0.0 || self.validation.impact_cap_multiplier > 1.0 {
            anyhow::bail!("IMPACT_CAP_MULTIPLIER must be between 0.0 and 1.0");
        }
        if self.validation.min_liquidity_usd < 0.0 {
            anyhow::bail!("MIN_LIQUIDITY_USD must be ≥ 0");
        }
        if self.validation.max_slippage < 0.0 || self.validation.max_slippage > 1.0 {
            anyhow::bail!("MAX_SLIPPAGE must be between 0.0 and 1.0");
        }

        // Guardrails
        if self.guardrails.max_concurrent_positions == 0 {
            anyhow::bail!("MAX_CONCURRENT_POSITIONS must be > 0");
        }
        if self.guardrails.max_advisor_positions > self.guardrails.max_concurrent_positions {
            anyhow::bail!("MAX_ADVISOR_POSITIONS cannot exceed MAX_CONCURRENT_POSITIONS");
        }

        // Network
        if self.network.advice_bus_port == 0 {
            anyhow::bail!("ADVICE_BUS_PORT must be > 0");
        }
        if self.network.decision_bus_port == 0 {
            anyhow::bail!("DECISION_BUS_PORT must be > 0");
        }
        if self.network.advice_bus_port == self.network.decision_bus_port {
            anyhow::bail!("ADVICE_BUS_PORT and DECISION_BUS_PORT must be different");
        }
        if self.network.wallet_pubkey.is_empty() {
            anyhow::bail!("WALLET_PUBKEY must be set for gRPC monitoring");
        }
        if self.network.yellowstone_endpoint.is_empty() {
            anyhow::bail!("YELLOWSTONE_ENDPOINT must be set");
        }
        if self.network.rpc_url.is_empty() {
            anyhow::bail!("RPC_URL must be set");
        }
        if self.network.telegram_bot_token.is_empty() {
            log::warn!("TELEGRAM_BOT_TOKEN is empty - notifications will be disabled");
        }
        if self.network.telegram_chat_id.is_empty() {
            log::warn!("TELEGRAM_CHAT_ID is empty - notifications will be disabled");
        }

        // Database
        if self.database.postgres_password.is_empty() {
            log::warn!("POSTGRES_PASSWORD is empty - this may cause connection issues");
        }

        // Cache
        if self.cache.mint_cache_capacity == 0 {
            anyhow::bail!("MINT_CACHE_CAPACITY must be > 0");
        }
        if self.cache.wallet_cache_capacity == 0 {
            anyhow::bail!("WALLET_CACHE_CAPACITY must be > 0");
        }

        Ok(())
    }
}

// Helper functions for environment variable parsing

fn get_env_string(key: &str, default: &str) -> Result<String> {
    Ok(env::var(key).unwrap_or_else(|_| default.to_string()))
}

fn get_env_u8(key: &str, default: u8) -> Result<u8> {
    env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .or(Some(default))
        .context(format!("Invalid {} value", key))
}

fn get_env_u16(key: &str, default: u16) -> Result<u16> {
    env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .or(Some(default))
        .context(format!("Invalid {} value", key))
}

fn get_env_u64(key: &str, default: u64) -> Result<u64> {
    env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .or(Some(default))
        .context(format!("Invalid {} value", key))
}

fn get_env_usize(key: &str, default: usize) -> Result<usize> {
    env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .or(Some(default))
        .context(format!("Invalid {} value", key))
}

fn get_env_f64(key: &str, default: f64) -> Result<f64> {
    env::var(key)
        .ok()
        .and_then(|s| s.parse().ok())
        .or(Some(default))
        .context(format!("Invalid {} value", key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    #[ignore] // Run this test separately: cargo test test_config_from_env_with_defaults -- --ignored
    fn test_config_from_env_with_defaults() {
        // Test loading config with all defaults (no .env file)
        // Clean up any env vars from other tests
        env::remove_var("MIN_DECISION_CONF");
        
        let config = Config::from_env().expect("Failed to load config");
        
        assert_eq!(config.decision.min_decision_conf, 75);
        assert_eq!(config.decision.min_copytrade_confidence, 70);
        assert_eq!(config.validation.fee_multiplier, 2.2);
        assert_eq!(config.guardrails.max_concurrent_positions, 3);
        assert_eq!(config.network.advice_bus_port, 45100);
        assert_eq!(config.network.decision_bus_port, 45110);
    }

    #[test]
    #[ignore] // Run this test separately: cargo test test_env_var_override -- --ignored
    fn test_env_var_override() {
        // Set env var
        env::set_var("MIN_DECISION_CONF", "80");
        let config = Config::from_env().expect("Failed to load config");
        assert_eq!(config.decision.min_decision_conf, 80);
        // Clean up immediately
        env::remove_var("MIN_DECISION_CONF");
        
        // Verify cleanup worked
        let config2 = Config::from_env().expect("Failed to load config");
        assert_eq!(config2.decision.min_decision_conf, 75);
    }

    #[test]
    fn test_config_validation_success() {
        let config = Config::from_env().expect("Failed to load config");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_validation_invalid_confidence() {
        let mut config = Config::from_env().expect("Failed to load config");
        config.decision.min_decision_conf = 150;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_invalid_multiplier() {
        let mut config = Config::from_env().expect("Failed to load config");
        config.validation.impact_cap_multiplier = 1.5;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_invalid_positions() {
        let mut config = Config::from_env().expect("Failed to load config");
        config.guardrails.max_advisor_positions = 5;
        config.guardrails.max_concurrent_positions = 3;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_config_validation_same_ports() {
        let mut config = Config::from_env().expect("Failed to load config");
        config.network.advice_bus_port = 45100;
        config.network.decision_bus_port = 45100;
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_postgres_connection_string() {
        let db_config = DatabaseConfig {
            postgres_host: "localhost".to_string(),
            postgres_port: 5432,
            postgres_user: "testuser".to_string(),
            postgres_password: "testpass".to_string(),
            postgres_db: "testdb".to_string(),
            sqlite_path: PathBuf::from("./test.db"),
        };

        let conn_str = db_config.postgres_connection_string();
        assert!(conn_str.contains("host=localhost"));
        assert!(conn_str.contains("port=5432"));
        assert!(conn_str.contains("user=testuser"));
        assert!(conn_str.contains("password=testpass"));
        assert!(conn_str.contains("dbname=testdb"));
    }
}
