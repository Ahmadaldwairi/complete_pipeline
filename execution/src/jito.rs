use anyhow::{anyhow, Result};
use jito_sdk_rust::JitoJsonRpcSDK;
use solana_sdk::{
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use base64::{Engine as _, engine::general_purpose};
use log::{info, debug, warn, error};
use serde_json::json;
use std::sync::Mutex as StdMutex;
use std::time::{Instant, Duration};

// Global rate limiter: tracks last bundle submission time
// Using std::sync::Mutex for static variable (simpler than tokio::sync::Mutex)
static LAST_BUNDLE_TIME: StdMutex<Option<Instant>> = StdMutex::new(None);

/// Jito client wrapper for submitting bundles to the block engine
pub struct JitoClient {
    sdk: JitoJsonRpcSDK,
    tip_accounts: Vec<Pubkey>,
}

impl JitoClient {
    /// Create a new Jito client
    /// 
    /// # Arguments
    /// * `block_engine_url` - Jito block engine endpoint (e.g., "https://mainnet.block-engine.jito.wtf/api/v1")
    /// * `uuid` - Optional UUID for rate-limited access
    pub async fn new(block_engine_url: &str, uuid: Option<String>) -> Result<Self> {
        info!("🔧 Initializing Jito client: {}", block_engine_url);
        
        let sdk = JitoJsonRpcSDK::new(block_engine_url, uuid);
        
        // Fetch tip accounts on initialization
        let tip_accounts_response = sdk.get_tip_accounts().await
            .map_err(|e| anyhow!("Failed to fetch Jito tip accounts: {}", e))?;
        
        debug!("Jito tip accounts response: {:?}", tip_accounts_response);
        
        // Try to parse tip accounts from response
        let tip_accounts: Vec<Pubkey> = if let Some(result) = tip_accounts_response.get("result") {
            // Check if result is directly an array
            if let Some(arr) = result.as_array() {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .filter_map(|s| s.parse().ok())
                    .collect()
            } else {
                // Fallback: Use hardcoded Jito tip accounts (these are public and well-known)
                warn!("⚠️ Could not parse dynamic tip accounts, using hardcoded addresses");
                vec![
                    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5".parse()?,
                    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe".parse()?,
                    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY".parse()?,
                    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49".parse()?,
                    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh".parse()?,
                    "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt".parse()?,
                    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL".parse()?,
                    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT".parse()?,
                ]
            }
        } else {
            // Fallback to hardcoded
            warn!("⚠️ No 'result' field in response, using hardcoded Jito tip accounts");
            vec![
                "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5".parse()?,
                "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe".parse()?,
                "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY".parse()?,
                "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49".parse()?,
                "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh".parse()?,
                "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt".parse()?,
                "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL".parse()?,
                "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT".parse()?,
            ]
        };
        
        if tip_accounts.is_empty() {
            return Err(anyhow!("No Jito tip accounts available"));
        }
        
        info!("✅ Jito client initialized with {} tip accounts", tip_accounts.len());
        
        Ok(Self {
            sdk,
            tip_accounts,
        })
    }
    
    /// Get a random tip account for Jito tips
    pub fn get_random_tip_account(&self) -> Result<Pubkey> {
        use rand::seq::SliceRandom;
        
        self.tip_accounts
            .choose(&mut rand::thread_rng())
            .copied()
            .ok_or_else(|| anyhow!("No tip accounts available"))
    }
    
    /// Get dynamic tip amount based on recent bundle activity
    /// 
    /// Returns the tip amount in lamports for a given percentile:
    /// - 25th percentile: Low competition (saves money)
    /// - 50th percentile: Medium competition (balanced)
    /// - 75th percentile: High competition (faster inclusion)
    /// - 95th percentile: Ultra competition (mempool entries)
    pub async fn get_dynamic_tip(&self, percentile: f64) -> Result<u64> {
        debug!("📊 Fetching dynamic tip for {}th percentile", percentile);
        
        // Fetch recent tip floor percentiles from Jito
        let response = self.sdk.get_tip_accounts().await;
        
        // Parse percentiles if available, otherwise use smart defaults
        let tip_lamports = match response {
            Ok(data) => {
                // Try to extract tip percentiles from response
                if let Some(result) = data.get("result") {
                    if let Some(percentiles) = result.get("percentiles") {
                        // Get the requested percentile
                        if let Some(tip) = percentiles.get(percentile.to_string()) {
                            if let Some(amount) = tip.as_u64() {
                                debug!("✅ Got dynamic tip: {} lamports ({}th percentile)", amount, percentile);
                                return Ok(amount);
                            }
                        }
                    }
                }
                
                // Fallback to smart defaults based on percentile
                warn!("⚠️ Could not fetch dynamic tip, using defaults");
                self.get_default_tip_for_percentile(percentile)
            }
            Err(_) => {
                warn!("⚠️ Failed to fetch tip data, using defaults");
                self.get_default_tip_for_percentile(percentile)
            }
        };
        
        Ok(tip_lamports)
    }
    
    /// Get default tip amounts when dynamic fetching fails
    fn get_default_tip_for_percentile(&self, percentile: f64) -> u64 {
        match percentile {
            p if p <= 25.0 => 1_000,        // 0.000001 SOL (~$0.0002)
            p if p <= 50.0 => 10_000,       // 0.00001 SOL (~$0.002)
            p if p <= 75.0 => 100_000,      // 0.0001 SOL (~$0.02)
            p if p <= 95.0 => 500_000,      // 0.0005 SOL (~$0.10)
            _ => 1_000_000,                 // 0.001 SOL (~$0.20) for ultra competition
        }
    }
    
    /// Submit a single transaction as a bundle to Jito
    /// 
    /// Returns the bundle UUID for tracking
    pub async fn send_transaction_bundle(
        &self,
        transaction: &Transaction,
    ) -> Result<String> {
        info!("🚨 send_transaction_bundle() CALLED - starting bundle submission process");
        debug!("📦 Preparing bundle for submission...");
        
        // Serialize transaction to base64
        let serialized_tx = general_purpose::STANDARD.encode(
            bincode::serialize(transaction)?
        );
        
        // Prepare bundle (array of transactions)
        let transactions = json!([serialized_tx]);
        
        // Create parameters with encoding specification
        let params = json!([
            transactions,
            {
                "encoding": "base64"
            }
        ]);
        
        // GLOBAL rate limit protection: Jito free tier = 1 req/sec per IP
        // This ensures we NEVER send more than 1 request per 2 seconds system-wide
        info!("🔒 Checking global rate limit...");
        
        // Calculate wait time (if any) inside the lock, then release lock before sleeping
        let wait_time = {
            let mut last_time = LAST_BUNDLE_TIME.lock().unwrap();
            
            let wait_duration = if let Some(last) = *last_time {
                let elapsed = last.elapsed();
                let required_delay = Duration::from_secs(2);
                
                info!("⏱️  Last bundle was {:.2}s ago", elapsed.as_secs_f64());
                
                if elapsed < required_delay {
                    let wait = required_delay - elapsed;
                    info!("⏱️  Need to wait {:.2}s more", wait.as_secs_f64());
                    Some(wait)
                } else {
                    info!("⏱️  Sufficient time elapsed - no wait needed");
                    None
                }
            } else {
                info!("⏱️  First bundle submission - no delay needed");
                None
            };
            
            // Update last submission time NOW (before waiting)
            *last_time = Some(Instant::now());
            info!("📝 Updated last submission timestamp");
            
            wait_duration
        }; // Lock is released here
        
        // Now sleep if needed (outside the lock)
        if let Some(duration) = wait_time {
            info!("⏳ Waiting {:.2}s for rate limit...", duration.as_secs_f64());
            tokio::time::sleep(duration).await;
            info!("✅ Wait complete - proceeding with submission");
        }
        
        // Retry logic with exponential backoff (for non-rate-limit errors only)
        let max_retries = 3;
        let mut retry_delay = 1; // Start with 1 second
        
        for attempt in 1..=max_retries {
            // Submit bundle
            if attempt == 1 {
                info!("🚀 Submitting bundle to Jito...");
            } else {
                info!("🔄 Retry attempt {}/{} after {}s delay...", attempt, max_retries, retry_delay);
                tokio::time::sleep(tokio::time::Duration::from_secs(retry_delay)).await;
            }
            
            match self.sdk.send_bundle(Some(params.clone()), None).await {
                Ok(response) => {
                    // Debug: Log the full response
                    debug!("Jito response: {:?}", response);
                    
                    // Check for rate limit error - FAIL IMMEDIATELY (don't retry)
                    if let Some(error) = response.get("error") {
                        if let Some(message) = error.get("message") {
                            if message.as_str().unwrap_or("").contains("rate limited") {
                                warn!("⚠️ Rate limited! This should not happen with 1.1s delay.");
                                warn!("⚠️ Skipping this trade opportunity.");
                                return Err(anyhow!("Rate limited. Delay may not be enough or another process is using same IP."));
                            }
                        }
                        return Err(anyhow!("Jito error: {:?}", error));
                    }
                    
                    // Extract bundle UUID - try different response formats
                    let bundle_uuid = if let Some(result) = response.get("result") {
                        // Format 1: {"result": "uuid-string"}
                        if let Some(uuid_str) = result.as_str() {
                            uuid_str.to_string()
                        }
                        // Format 2: {"result": {"uuid": "uuid-string"}}
                        else if let Some(uuid_obj) = result.get("uuid") {
                            uuid_obj.as_str()
                                .ok_or_else(|| anyhow!("UUID field is not a string"))?
                                .to_string()
                        }
                        // Format 3: {"result": {"bundleId": "uuid-string"}}
                        else if let Some(bundle_id) = result.get("bundleId") {
                            bundle_id.as_str()
                                .ok_or_else(|| anyhow!("bundleId field is not a string"))?
                                .to_string()
                        }
                        else {
                            return Err(anyhow!("Failed to get bundle UUID from response. Response: {:?}", response));
                        }
                    } else {
                        return Err(anyhow!("No 'result' field in Jito response. Full response: {:?}", response));
                    };
                    
                    info!("✅ Bundle submitted with UUID: {}", bundle_uuid);
                    return Ok(bundle_uuid);
                }
                Err(e) => {
                    warn!("❌ Bundle submission failed on attempt {}/{}: {}", attempt, max_retries, e);
                    if attempt < max_retries {
                        retry_delay *= 2;
                        continue;
                    } else {
                        return Err(e.into());
                    }
                }
            }
        }
        
        Err(anyhow!("Failed to submit bundle after {} attempts", max_retries))
    }
    
    /// Check the status of a bundle
    pub async fn get_bundle_status(&self, bundle_uuid: &str) -> Result<serde_json::Value> {
        self.sdk.get_in_flight_bundle_statuses(vec![bundle_uuid.to_string()])
            .await
            .map_err(|e| anyhow!("Failed to get bundle status: {}", e))
    }
    
    /// Wait for bundle to land on-chain
    /// 
    /// Returns true if landed successfully, false if failed
    pub async fn wait_for_bundle_confirmation(
        &self,
        bundle_uuid: &str,
        max_attempts: u32,
    ) -> Result<bool> {
        debug!("⏳ Waiting for bundle confirmation...");
        
        for attempt in 1..=max_attempts {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            
            let status_response = self.get_bundle_status(bundle_uuid).await?;
            
            if let Some(result) = status_response.get("result") {
                if let Some(value) = result.get("value") {
                    if let Some(statuses) = value.as_array() {
                        if let Some(bundle_status) = statuses.first() {
                            if let Some(status) = bundle_status.get("status") {
                                match status.as_str() {
                                    Some("Landed") => {
                                        info!("✅ Bundle landed on-chain!");
                                        return Ok(true);
                                    },
                                    Some("Pending") => {
                                        debug!("📊 Bundle pending (attempt {}/{})", attempt, max_attempts);
                                    },
                                    Some("Failed") | Some("Invalid") => {
                                        warn!("❌ Bundle failed: {}", status.as_str().unwrap_or("unknown"));
                                        return Ok(false);
                                    },
                                    Some(s) => {
                                        debug!("📊 Bundle status: {} (attempt {}/{})", s, attempt, max_attempts);
                                    },
                                    None => {
                                        debug!("📊 Unknown status (attempt {}/{})", attempt, max_attempts);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        
        warn!("⏱️ Bundle confirmation timeout after {} attempts", max_attempts);
        Ok(false)
    }
    
    /// Get final bundle status (confirmed/finalized)
    pub async fn get_final_bundle_status(&self, bundle_uuid: &str) -> Result<BundleStatus> {
        let status_response = self.sdk.get_bundle_statuses(vec![bundle_uuid.to_string()]).await?;
        
        let bundle_status = status_response
            .get("result")
            .and_then(|result| result.get("value"))
            .and_then(|value| value.as_array())
            .and_then(|statuses| statuses.first())
            .ok_or_else(|| anyhow!("Failed to parse bundle status"))?;
        
        let confirmation_status = bundle_status
            .get("confirmation_status")
            .and_then(|s| s.as_str())
            .map(String::from);
        
        let transactions = bundle_status
            .get("transactions")
            .and_then(|t| t.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            });
        
        let err = bundle_status.get("err").cloned();
        
        Ok(BundleStatus {
            confirmation_status,
            err,
            transactions,
        })
    }
}

#[derive(Debug, Clone)]
pub struct BundleStatus {
    pub confirmation_status: Option<String>,
    pub err: Option<serde_json::Value>,
    pub transactions: Option<Vec<String>>,
}

impl BundleStatus {
    pub fn is_confirmed(&self) -> bool {
        matches!(
            self.confirmation_status.as_deref(),
            Some("confirmed") | Some("finalized")
        )
    }
    
    pub fn has_error(&self) -> bool {
        self.err.is_some() && !self.err.as_ref().unwrap()["Ok"].is_null()
    }
    
    pub fn get_signature(&self) -> Option<&str> {
        self.transactions
            .as_ref()
            .and_then(|txs| txs.first())
            .map(|s| s.as_str())
    }
}
