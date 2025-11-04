use anyhow::Result;
use log::info;
use std::env;

fn get_env(key: &str, default: &str) -> Result<String> {
    Ok(env::var(key).unwrap_or_else(|_| default.to_string()))
}

fn get_env_u16(key: &str, default: u16) -> Result<u16> {
    Ok(env::var(key)
        .unwrap_or_else(|_| default.to_string())
        .parse()?)
}

fn get_env_u64(key: &str, default: u64) -> Result<u64> {
    Ok(env::var(key)
        .unwrap_or_else(|_| default.to_string())
        .parse()?)
}

fn get_env_f64(key: &str, default: f64) -> Result<f64> {
    Ok(env::var(key)
        .unwrap_or_else(|_| default.to_string())
        .parse()?)
}

fn get_env_usize(key: &str, default: usize) -> Result<usize> {
    Ok(env::var(key)
        .unwrap_or_else(|_| default.to_string())
        .parse()?)
}

#[derive(Debug, Clone)]
pub struct Config {
    pub rpc: RpcConfig,
    pub udp: UdpConfig,
    pub monitoring: MonitoringConfig,
    pub thresholds: ThresholdConfig,
    pub logging: LoggingConfig,
    pub performance: PerformanceConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug, Clone)]
pub struct RpcConfig {
    pub url: String,
    pub ws_url: String,
}

#[derive(Debug, Clone)]
pub struct UdpConfig {
    pub brain_port: u16,                 // Brain listens for signals (45100, 45120, 45131)
    pub watch_listen_port: u16,          // Mempool listens for watch requests (45130)
    pub brain_confirmation_port: u16,    // Brain listens for post-confirmation intelligence (45131)
    pub executor_confirmed_port: u16,    // Executor listens for TxConfirmed (45132)
    pub bind_address: String,
}

#[derive(Debug, Clone)]
pub struct MonitoringConfig {
    pub heat_update_interval_secs: u64,
    pub hot_signal_cooldown_ms: u64,
    pub transaction_window_secs: u64,
}

#[derive(Debug, Clone)]
pub struct ThresholdConfig {
    pub whale_threshold_sol: f64,
    pub bot_repeat_threshold: usize,
    pub heat_index_threshold: u8,
}

#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub level: String,
    pub hot_signals_log: String,
    pub heat_index_log: String,
    pub transaction_log: String,
}

#[derive(Debug, Clone)]
pub struct PerformanceConfig {
    pub worker_threads: usize,
    pub buffer_size: usize,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub sqlite_path: String,
    pub alpha_wallet_update_interval_secs: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenv::dotenv().ok();

        let config = Self {
            rpc: RpcConfig {
                url: get_env("SOLANA_RPC_URL", "https://api.mainnet-beta.solana.com")?,
                ws_url: get_env("SOLANA_RPC_WS_URL", "wss://api.mainnet-beta.solana.com")?,
            },
            udp: UdpConfig {
                brain_port: get_env_u16("BRAIN_UDP_PORT", 45120)?,
                watch_listen_port: get_env_u16("WATCH_LISTEN_PORT", 45130)?,
                brain_confirmation_port: get_env_u16("BRAIN_CONFIRMATION_PORT", 45131)?,
                executor_confirmed_port: get_env_u16("EXECUTOR_CONFIRMED_PORT", 45132)?,
                bind_address: get_env("UDP_BIND_ADDRESS", "127.0.0.1")?,
            },
            monitoring: MonitoringConfig {
                heat_update_interval_secs: get_env_u64("HEAT_UPDATE_INTERVAL_SECS", 5)?,
                hot_signal_cooldown_ms: get_env_u64("HOT_SIGNAL_COOLDOWN_MS", 1000)?,
                transaction_window_secs: get_env_u64("TRANSACTION_WINDOW_SECS", 10)?,
            },
            thresholds: ThresholdConfig {
                whale_threshold_sol: get_env_f64("WHALE_THRESHOLD_SOL", 10.0)?,
                bot_repeat_threshold: get_env_usize("BOT_REPEAT_THRESHOLD", 3)?,
                heat_index_threshold: get_env_u8("HEAT_INDEX_THRESHOLD", 70)?,
            },
            logging: LoggingConfig {
                level: get_env("LOG_LEVEL", "info")?,
                hot_signals_log: get_env("HOT_SIGNALS_LOG", "./logs/mempool_hot_signals.log")?,
                heat_index_log: get_env("HEAT_INDEX_LOG", "./logs/mempool_heat_index.log")?,
                transaction_log: get_env("TRANSACTION_LOG", "./logs/mempool_transactions.log")?,
            },
            performance: PerformanceConfig {
                worker_threads: get_env_usize("WORKER_THREADS", 4)?,
                buffer_size: get_env_usize("BUFFER_SIZE", 10000)?,
            },
            database: DatabaseConfig {
                sqlite_path: get_env("SQLITE_DB_PATH", "../data-mining/data/collector.db")?,
                alpha_wallet_update_interval_secs: get_env_u64("ALPHA_WALLET_UPDATE_INTERVAL_SECS", 60)?,
            },
        };

        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<()> {
        if self.thresholds.whale_threshold_sol <= 0.0 {
            anyhow::bail!("WHALE_THRESHOLD_SOL must be > 0");
        }

        if self.thresholds.bot_repeat_threshold < 2 {
            anyhow::bail!("BOT_REPEAT_THRESHOLD must be >= 2");
        }

        if self.thresholds.heat_index_threshold > 100 {
            anyhow::bail!("HEAT_INDEX_THRESHOLD must be <= 100");
        }

        if self.monitoring.heat_update_interval_secs == 0 {
            anyhow::bail!("HEAT_UPDATE_INTERVAL_SECS must be > 0");
        }

        Ok(())
    }

    pub fn print_startup_info(&self) {
        info!("🔥 MEMPOOL WATCHER SERVICE");
        info!("⏰ {}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        info!("📡 RPC: {}", self.rpc.url);
        info!("🌊 WebSocket: {}", self.rpc.ws_url);
        info!("📤 Brain UDP: port {}", self.udp.brain_port);
        info!("📤 Brain Confirmation UDP: port {}", self.udp.brain_confirmation_port);
        info!("💾 SQLite DB: {}", self.database.sqlite_path);
        info!("⏱️  Heat update: every {}s", self.monitoring.heat_update_interval_secs);
        info!("👥 Alpha wallet update: every {}s", self.database.alpha_wallet_update_interval_secs);
        info!("🐋 Whale threshold: {} SOL", self.thresholds.whale_threshold_sol);
        info!("🤖 Bot detection: {} repeat txs", self.thresholds.bot_repeat_threshold);
        info!("🔥 Heat threshold: {}", self.thresholds.heat_index_threshold);
        info!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    }
}

fn get_env_u8(key: &str, default: u8) -> Result<u8> {
    Ok(env::var(key)
        .unwrap_or_else(|_| default.to_string())
        .parse()?)
}
