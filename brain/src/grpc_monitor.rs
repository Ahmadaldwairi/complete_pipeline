//! 🔗 gRPC Monitor - Direct Yellowstone Connection for Position Tracking
//!
//! Brain's direct gRPC connection provides real-time monitoring for:
//! 1. Trading wallet - transaction confirmations
//! 2. Bonding curve accounts - price updates for IN_POSITION tokens
//! 3. Program logs - trade activity on tracked tokens
//!
//! This complements UDP signals from data-mining:
//! - UDP: Hot signals, new launches, wallet activity triggers
//! - gRPC: Position-specific monitoring, confirmations, price updates
//!
//! Key difference from data-mining:
//! - Data-mining subscribes to ALL pump.fun transactions (discovery mode)
//! - Brain subscribes ONLY to specific accounts it's tracking (monitoring mode)

use anyhow::{Context, Result};
use futures::StreamExt;
use log::{debug, error, info, warn};
use solana_sdk::pubkey::Pubkey;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::geyser::{
    subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest,
    SubscribeRequestFilterAccounts, SubscribeRequestFilterBlocksMeta,
    SubscribeRequestFilterSlots,
};

/// Configuration for Yellowstone gRPC connection
#[derive(Debug, Clone)]
pub struct GrpcConfig {
    pub endpoint: String,
    pub x_token: Option<String>,
    pub max_retries: u32,
    pub retry_delay_secs: u64,
}

impl GrpcConfig {
    /// Load from environment variables
    pub fn from_env() -> Result<Self> {
        Ok(Self {
            endpoint: std::env::var("YELLOWSTONE_ENDPOINT")
                .unwrap_or_else(|_| "http://127.0.0.1:10000".to_string()),
            x_token: std::env::var("YELLOWSTONE_TOKEN").ok(),
            max_retries: 5,
            retry_delay_secs: 5,
        })
    }
}

/// Account subscription for position monitoring
#[derive(Debug, Clone)]
pub struct AccountSubscription {
    pub account: Pubkey,
    pub owner: Option<Pubkey>, // Optional: filter by program owner
    pub description: String,    // For logging (e.g., "wallet", "bonding_curve:MINT...")
}

/// gRPC monitor handles Yellowstone connection and account subscriptions
pub struct GrpcMonitor {
    config: GrpcConfig,
    subscriptions: Arc<RwLock<Vec<AccountSubscription>>>,
    wallet_pubkey: Pubkey,
    pump_program_id: Pubkey,
}

impl GrpcMonitor {
    /// Create new gRPC monitor
    pub fn new(config: GrpcConfig, wallet_pubkey: Pubkey, pump_program_id: Pubkey) -> Self {
        Self {
            config,
            subscriptions: Arc::new(RwLock::new(Vec::new())),
            wallet_pubkey,
            pump_program_id,
        }
    }

    /// Add trading wallet to subscriptions (called at startup)
    pub async fn subscribe_wallet(&self) {
        let mut subs = self.subscriptions.write().await;
        subs.push(AccountSubscription {
            account: self.wallet_pubkey,
            owner: None,
            description: "trading_wallet".to_string(),
        });
        info!(
            "📝 Added wallet subscription: {}",
            self.wallet_pubkey.to_string()
        );
    }

    /// Add position's bonding curve to subscriptions (called when entering position)
    pub async fn subscribe_position(&self, mint: Pubkey, bonding_curve_pda: Pubkey) {
        let mut subs = self.subscriptions.write().await;

        // Check if already subscribed
        if subs.iter().any(|s| s.account == bonding_curve_pda) {
            debug!(
                "⏭️  Already subscribed to bonding curve for mint {}",
                &mint.to_string()[..12]
            );
            return;
        }

        subs.push(AccountSubscription {
            account: bonding_curve_pda,
            owner: Some(self.pump_program_id),
            description: format!("bonding_curve:{}", &mint.to_string()[..12]),
        });

        info!(
            "📝 Added bonding curve subscription for mint: {}",
            &mint.to_string()[..12]
        );
    }

    /// Remove position subscription (called when position closed)
    pub async fn unsubscribe_position(&self, bonding_curve_pda: Pubkey) {
        let mut subs = self.subscriptions.write().await;
        subs.retain(|s| s.account != bonding_curve_pda);
        debug!("🗑️  Removed bonding curve subscription");
    }

    /// Build subscription request from current subscriptions
    async fn build_subscription_request(&self) -> SubscribeRequest {
        let subs = self.subscriptions.read().await;
        let mut accounts = HashMap::new();

        // Build account filter
        let account_pubkeys: Vec<String> = subs.iter().map(|s| s.account.to_string()).collect();

        if !account_pubkeys.is_empty() {
            accounts.insert(
                "brain_accounts".to_string(),
                SubscribeRequestFilterAccounts {
                    account: account_pubkeys,
                    owner: vec![], // Don't filter by owner (we have wallet + bonding curves)
                    filters: vec![],
                    nonempty_txn_signature: None, // Don't filter by transaction signature
                },
            );
        }

        // Add slot updates for tracking
        let mut slots = HashMap::new();
        slots.insert(
            "slots".to_string(),
            SubscribeRequestFilterSlots {
                filter_by_commitment: None,
                interslot_updates: Some(false),
            },
        );

        // Add block metadata
        let mut blocks_meta = HashMap::new();
        blocks_meta.insert(
            "blocks_meta".to_string(),
            SubscribeRequestFilterBlocksMeta {},
        );

        SubscribeRequest {
            accounts,
            slots,
            transactions: HashMap::new(),
            transactions_status: HashMap::new(),
            blocks: HashMap::new(),
            blocks_meta,
            entry: HashMap::new(),
            commitment: Some(CommitmentLevel::Confirmed as i32),
            accounts_data_slice: vec![],
            ping: None,
            from_slot: None,
        }
    }

    /// Connect to Yellowstone and start monitoring
    pub async fn start(
        &self,
        update_handler: impl Fn(UpdateOneof) -> Result<()> + Send + Sync + 'static,
    ) -> Result<()> {
        let mut attempts = 0;

        loop {
            attempts += 1;

            // Build subscription request
            let request = self.build_subscription_request().await;
            let account_count = self.subscriptions.read().await.len();

            info!("🔗 Connecting to Yellowstone gRPC at {}", self.config.endpoint);
            info!("📊 Subscribing to {} accounts", account_count);

            match GeyserGrpcClient::build_from_shared(self.config.endpoint.clone()) {
                Ok(mut client_builder) => {
                    // Add x-token if provided
                    if let Some(token) = &self.config.x_token {
                        client_builder = client_builder.x_token(Some(token.clone()))?;
                    }

                    match client_builder.connect().await {
                        Ok(mut client) => {
                            info!("✅ Connected to Yellowstone gRPC");

                            // Subscribe
                            let (_subscribe_tx, mut stream) = client
                                .subscribe_with_request(Some(request))
                                .await
                                .context("Failed to create subscription")?;

                            info!("📡 gRPC stream established");

                            // Process stream
                            while let Some(message) = stream.next().await {
                                match message {
                                    Ok(msg) => {
                                        if let Some(update) = msg.update_oneof {
                                            if let Err(e) = update_handler(update) {
                                                warn!("⚠️  Error handling gRPC update: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("❌ gRPC stream error: {}", e);
                                        break; // Reconnect
                                    }
                                }
                            }

                            warn!("⚠️  gRPC stream ended, reconnecting...");
                        }
                        Err(e) => {
                            if attempts >= self.config.max_retries {
                                return Err(anyhow::anyhow!(
                                    "Failed to connect after {} attempts: {}",
                                    self.config.max_retries,
                                    e
                                ));
                            }
                            warn!(
                                "⚠️  Connection attempt {}/{} failed: {}. Retrying in {}s...",
                                attempts,
                                self.config.max_retries,
                                e,
                                self.config.retry_delay_secs
                            );
                            sleep(Duration::from_secs(self.config.retry_delay_secs)).await;
                        }
                    }
                }
                Err(e) => {
                    return Err(anyhow::anyhow!("Failed to build gRPC client: {}", e));
                }
            }

            // Brief delay before reconnect
            sleep(Duration::from_secs(2)).await;
            attempts = 0; // Reset on successful connection
        }
    }

    /// Get current subscription count
    pub async fn subscription_count(&self) -> usize {
        self.subscriptions.read().await.len()
    }
}
