// Emoji Map Loader - Centralized emoji configuration
// Loads from files/emoji_map.toml

use once_cell::sync::Lazy;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct EmojiMap {
    pub system: SystemEmojis,
    pub trading: TradingEmojis,
    pub advice_bus: AdviceBusEmojis,
    pub errors: ErrorEmojis,
    pub monitoring: MonitoringEmojis,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SystemEmojis {
    pub startup: String,
    pub config: String,
    pub database: String,
    pub network: String,
    pub wallet: String,
    pub shutdown: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TradingEmojis {
    pub launch_detected: String,
    pub entry_signal: String,
    pub position_entered: String,
    pub position_opened: String,
    pub exit_triggered: String,
    pub exit_completed: String,
    pub strategy_matched: String,
    pub profit_recorded: String,
    pub loss_recorded: String,
    pub mempool_check: String,
    pub volume_check: String,
    pub buyer_check: String,
    pub price_check: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct AdviceBusEmojis {
    pub listening: String,
    pub advisory_sent: String,
    pub advisory_received: String,
    pub hold_extended: String,
    pub exit_widened: String,
    pub urgent_exit: String,
    pub advisory_rejected: String,
    pub advisory_applied: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ErrorEmojis {
    pub warning: String,
    pub error: String,
    pub retry: String,
    pub success: String,
    pub failed: String,
    pub timeout: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct MonitoringEmojis {
    pub heartbeat: String,
    pub status: String,
    pub metrics: String,
    pub alert: String,
}

impl EmojiMap {
    fn load() -> Self {
        let content = fs::read_to_string("files/emoji_map.toml")
            .unwrap_or_else(|e| {
                eprintln!("Failed to load emoji_map.toml: {}. Using defaults.", e);
                Self::default_toml()
            });
        
        toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!("Failed to parse emoji_map.toml: {}. Using defaults.", e);
            toml::from_str(&Self::default_toml()).unwrap()
        })
    }

    fn default_toml() -> String {
        r#"
[system]
startup = "🚀"
config = "⚙️"
database = "💾"
network = "📡"
wallet = "👛"
shutdown = "🛑"

[trading]
launch_detected = "👀"
entry_signal = "🎯"
position_entered = "💰"
position_opened = "📈"
exit_triggered = "🔴"
exit_completed = "📉"
strategy_matched = "📊"
profit_recorded = "💸"
loss_recorded = "💔"
mempool_check = "🔍"
volume_check = "📦"
buyer_check = "👥"
price_check = "💵"

[advice_bus]
listening = "👂"
advisory_sent = "📤"
advisory_received = "📥"
hold_extended = "⏰"
exit_widened = "🎨"
urgent_exit = "🚨"
advisory_rejected = "🔇"
advisory_applied = "✅"

[errors]
warning = "⚠️"
error = "❌"
retry = "🔧"
success = "✅"
failed = "❗"
timeout = "⏳"

[monitoring]
heartbeat = "💓"
status = "📋"
metrics = "📈"
alert = "🔔"
"#.to_string()
    }
}

// Global emoji map instance
pub static EMOJIS: Lazy<EmojiMap> = Lazy::new(|| EmojiMap::load());

// Convenience macro for logging with emojis
#[macro_export]
macro_rules! log_info {
    ($emoji:expr, $($arg:tt)*) => {
        log::info!("{} {}", $emoji, format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_warn {
    ($emoji:expr, $($arg:tt)*) => {
        log::warn!("{} {}", $emoji, format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_error {
    ($emoji:expr, $($arg:tt)*) => {
        log::error!("{} {}", $emoji, format!($($arg)*))
    };
}

#[macro_export]
macro_rules! log_debug {
    ($emoji:expr, $($arg:tt)*) => {
        log::debug!("{} {}", $emoji, format!($($arg)*))
    };
}
