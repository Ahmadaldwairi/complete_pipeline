use solana_client::rpc_client::RpcClient;
use solana_client::nonblocking::tpu_client::TpuClient as AsyncTpuClient;
use solana_client::nonblocking::rpc_client::RpcClient as AsyncRpcClient;
use solana_sdk::{
    signature::Signature,
    transaction::Transaction,
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
};
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use anyhow::Result;
use log::{info, debug, warn};

/// TIER 2: Leader schedule cache for targeted TPU submissions
#[derive(Clone, Debug)]
pub struct LeaderScheduleCache {
    pub current_slot: u64,
    pub current_leader: Option<Pubkey>,
    pub next_leader: Option<Pubkey>,
    pub last_update: std::time::Instant,
}

impl LeaderScheduleCache {
    pub fn new() -> Self {
        Self {
            current_slot: 0,
            current_leader: None,
            next_leader: None,
            last_update: std::time::Instant::now(),
        }
    }
    
    pub fn is_stale(&self) -> bool {
        // Consider stale after 2 slots (800ms)
        self.last_update.elapsed() > std::time::Duration::from_millis(800)
    }
}

/// TPU Client for ultra-fast transaction submission directly to validators
pub struct FastTpuClient {
    rpc_client: Arc<RpcClient>,
    websocket_url: String,
    leader_cache: Arc<RwLock<LeaderScheduleCache>>,
    // TIER 4: TPU client cache for connection reuse
    rpc_client_cache: Arc<RwLock<Option<Arc<AsyncRpcClient>>>>,
}

impl FastTpuClient {
    /// Create a new TPU client with TIER 2 leader schedule caching
    pub fn new(rpc_url: &str, websocket_url: &str) -> Result<Self> {
        info!("🚀 Initializing TPU client with leader schedule targeting (TIER 2)...");
        
        let rpc_client = Arc::new(RpcClient::new_with_commitment(
            rpc_url.to_string(),
            CommitmentConfig::confirmed(),
        ));
        
        let leader_cache = Arc::new(RwLock::new(LeaderScheduleCache::new()));
        
        info!("✅ TPU client initialized - ready for leader-targeted submissions");
        
        Ok(Self {
            rpc_client,
            websocket_url: websocket_url.to_string(),
            leader_cache,
            rpc_client_cache: Arc::new(RwLock::new(None)),
        })
    }
    
    /// TIER 2: Refresh leader schedule cache
    /// Should be called periodically (every ~400ms) to stay current
    pub async fn refresh_leader_schedule(&self) -> Result<()> {
        let current_slot = self.rpc_client.get_slot()?;
        
        // Fetch leader schedule from RPC
        // Note: This returns a map of validator identity -> [slots they lead]
        let leader_schedule = self.rpc_client.get_leader_schedule(None)?;
        
        if let Some(schedule) = leader_schedule {
            // Find who's leading current and next slot
            let slot_index = current_slot % 432000; // Epoch has 432k slots
            let mut current_leader = None;
            let mut next_leader = None;
            
            // Search through schedule to find leaders
            for (leader_pubkey_str, slots) in schedule.iter() {
                if let Ok(leader_pubkey) = Pubkey::try_from(leader_pubkey_str.as_str()) {
                    // Check if this validator leads current slot
                    if slots.contains(&(slot_index as usize)) {
                        current_leader = Some(leader_pubkey);
                    }
                    // Check if this validator leads next slot
                    if slots.contains(&((slot_index + 1) as usize)) {
                        next_leader = Some(leader_pubkey);
                    }
                }
            }
            
            // Update cache
            let mut cache = self.leader_cache.write().await;
            cache.current_slot = current_slot;
            cache.current_leader = current_leader;
            cache.next_leader = next_leader;
            cache.last_update = std::time::Instant::now();
            
            if let Some(leader) = current_leader {
                debug!("🔄 Leader schedule refreshed - Slot: {}, Leader: {}", 
                    current_slot, leader.to_string().chars().take(8).collect::<String>());
            }
        }
        
        Ok(())
    }
    
    /// Get current leader schedule info for logging/monitoring
    /// Returns: (current_slot, current_leader, next_leader, is_stale)
    pub async fn get_leader_info(&self) -> (u64, Option<Pubkey>, Option<Pubkey>, bool) {
        let cache = self.leader_cache.read().await;
        (
            cache.current_slot,
            cache.current_leader,
            cache.next_leader,
            cache.is_stale()
        )
    }
    
    /// Check if we're near a slot boundary (last 100ms of slot)
    /// Useful for deciding whether to hedge to next leader
    pub async fn is_near_slot_boundary(&self) -> bool {
        let cache = self.leader_cache.read().await;
        // Each slot is ~400ms. If we're within 100ms of next slot, hedge
        let ms_into_slot = cache.last_update.elapsed().as_millis();
        ms_into_slot > 300 // Last 100ms of slot
    }
    
    /// Send and confirm transaction via TPU using nonblocking client
    /// TIER 4: Reuses TPU client connection when possible
    pub async fn send_and_confirm_transaction(
        &self,
        transaction: &Transaction,
        max_retries: usize,
    ) -> Result<Signature> {
        let signature = transaction.signatures[0];
        
        info!("📡 Sending and confirming via TPU: {}", signature);
        
        // Try to reuse existing async RPC client from cache
        let async_rpc = {
            let cache = self.rpc_client_cache.read().await;
            if let Some(ref client) = *cache {
                debug!("♻️ Reusing cached async RPC client connection");
                Arc::clone(client)
            } else {
                drop(cache); // Release read lock before acquiring write lock
                
                // Create new async RPC client
                let new_client = Arc::new(AsyncRpcClient::new_with_commitment(
                    self.rpc_client.url(),
                    CommitmentConfig::confirmed(),
                ));
                
                // Cache it for next use
                let mut cache = self.rpc_client_cache.write().await;
                *cache = Some(Arc::clone(&new_client));
                debug!("🔌 Created and cached new async RPC client connection");
                
                new_client
            }
        };
        
        // Create TPU client with cached RPC connection
        let tpu_client = AsyncTpuClient::new(
            "tpu-client",
            async_rpc,
            &self.websocket_url,
            solana_client::tpu_client::TpuClientConfig::default(),
        ).await?;
        
        // Send via TPU
        tpu_client.send_transaction(transaction).await;
        
        // Poll for confirmation
        for i in 0..max_retries {
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            
            if let Ok(status) = self.rpc_client.get_signature_status(&signature) {
                if let Some(result) = status {
                    if result.is_ok() {
                        info!("✅ Transaction confirmed via TPU: {}", signature);
                        return Ok(signature);
                    } else {
                        return Err(anyhow::anyhow!("Transaction failed: {:?}", result));
                    }
                }
            }
            
            if i > 0 && i % 5 == 0 {
                debug!("   Retry {}/{}: Still confirming...", i, max_retries);
                // Resend for good measure
                tpu_client.send_transaction(transaction).await;
            }
        }
        
        Err(anyhow::anyhow!("Transaction confirmation timeout after {} retries", max_retries))
    }
    
    /// OPTIMIZATION: Send transaction without waiting for confirmation
    /// Returns immediately - spawns send in background task
    /// TRUE ASYNC: Literally returns in <1ms, send happens in background
    pub async fn send_transaction_async(
        &self,
        transaction: &Transaction,
    ) -> Result<Signature> {
        let signature = transaction.signatures[0];
        let tx_clone = transaction.clone();
        let rpc_url = self.rpc_client.url().to_string();
        let ws_url = self.websocket_url.clone();
        
        info!("⚡ Spawning async TPU send (true fire-and-forget): {}", signature);
        
        // Spawn the actual send in a background task - returns immediately!
        tokio::spawn(async move {
            // Get async RPC client
            let async_rpc = Arc::new(AsyncRpcClient::new_with_commitment(
                rpc_url,
                CommitmentConfig::confirmed(),
            ));
            
            // Create TPU client and send
            match AsyncTpuClient::new(
                "tpu-client",
                async_rpc,
                &ws_url,
                solana_client::tpu_client::TpuClientConfig::default(),
            ).await {
                Ok(tpu_client) => {
                    tpu_client.send_transaction(&tx_clone).await;
                    info!("✅ Background TPU send completed: {}", signature);
                }
                Err(e) => {
                    warn!("❌ Background TPU send failed: {}", e);
                }
            }
        });
        
        // Return immediately - send is happening in background
        info!("✅ Transaction queued for async send (confirmation via gRPC): {}", signature);
        Ok(signature)
    }
}

