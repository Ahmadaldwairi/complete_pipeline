use serde::{Deserialize, Serialize};
use std::fs;
use anyhow::{Context, Result};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    pub grpc: GrpcConfig,
    pub programs: ProgramConfig,
    pub database: DatabaseConfig,
    pub checkpoint: CheckpointConfig,
    pub windows: WindowsConfig,
    pub monitoring: MonitoringConfig,
    pub rpc: RpcConfig,
    pub advice_bus: AdviceBusConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GrpcConfig {
    pub endpoint: String,
    pub max_retries: u32,
    pub retry_delay_secs: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProgramConfig {
    pub pump_program: String,
    pub spl_token_program: String,
    pub raydium_amm_v4: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DatabaseConfig {
    pub path: String,
    pub batch_size: usize,
    pub wal_mode: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CheckpointConfig {
    pub path: String,
    pub save_interval: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WindowsConfig {
    pub intervals: Vec<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MonitoringConfig {
    pub log_level: String,
    pub json_logs: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RpcConfig {
    pub endpoint: String,
    pub timeout_secs: u64,
}

impl Config {
    pub fn load(path: &str) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path))?;
        
        let config: Config = toml::from_str(&contents)
            .with_context(|| format!("Failed to parse config file: {}", path))?;
        
        Ok(config)
    }

    pub fn load_or_default() -> Result<Self> {
        // Try config.toml first, then config.example.toml
        Self::load("config.toml")
            .or_else(|_| Self::load("config.example.toml"))
            .context("Failed to load configuration")
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdviceBusConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub sustained_volume_threshold_secs: i64,
    pub sustained_volume_min_sol: f64,
    pub confidence: u8,
}
