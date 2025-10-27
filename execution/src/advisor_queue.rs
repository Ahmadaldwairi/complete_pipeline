///
/// Architecture: Collectors send LateOpportunity/CopyTrade → Queue → Process when idle
/// Key Features:
/// - Only processes when NOT in hot launch path
/// - Aborts queued entry if new launch detected
/// - Throttles advisor entries (max 1 per 30s)
/// - Tracks advisor vs launch positions separately

use std::collections::{VecDeque, HashSet};
use std::sync::Arc;
use std::time::{Instant, Duration};
use tokio::sync::RwLock;
use log::{info, debug, warn};

/// Advisor entry candidate (from LateOpportunity or CopyTrade)
#[derive(Debug, Clone)]
pub struct AdvisorCandidate {
    pub mint: String,              // Token address
    pub source: AdvisorSource,     // Where it came from
    pub score: u8,                 // 0-100
    pub queued_at: Instant,        // When it was queued
}

#[derive(Debug, Clone, PartialEq)]
pub enum AdvisorSource {
    LateOpportunity { horizon_sec: u16 },  // Token heating up after launch
    CopyTrade { wallet: String },          // Alpha wallet bought
    ExtendHold { horizon_sec: u16 },       // Volume surge, still has runway
}

/// Low-priority queue for advisor-driven entries
pub struct AdvisorQueue {
    queue: Arc<RwLock<VecDeque<AdvisorCandidate>>>,
    seen_mints: Arc<RwLock<HashSet<String>>>,  // Dedup per mint
    max_size: usize,
    last_entry_time: Arc<RwLock<Option<Instant>>>,
    advisor_position_count: Arc<RwLock<usize>>,  // Track advisor positions separately
}

impl AdvisorQueue {
    pub fn new(max_size: usize) -> Self {
        Self {
            queue: Arc::new(RwLock::new(VecDeque::with_capacity(max_size))),
            seen_mints: Arc::new(RwLock::new(HashSet::new())),
            max_size,
            last_entry_time: Arc::new(RwLock::new(None)),
            advisor_position_count: Arc::new(RwLock::new(0)),
        }
    }

    /// Add candidate to queue (dedupe by mint)
    pub async fn push(&self, candidate: AdvisorCandidate) -> bool {
        let mut queue = self.queue.write().await;
        let mut seen = self.seen_mints.write().await;

        // Dedup: skip if already queued or recently processed
        if seen.contains(&candidate.mint) {
            debug!("🔇 Advisor: Skipping duplicate mint {}", &candidate.mint[..12]);
            return false;
        }

        // Queue full: drop oldest if not at max
        if queue.len() >= self.max_size {
            if let Some(dropped) = queue.pop_front() {
                seen.remove(&dropped.mint);
                warn!("⚠️  Advisor queue full, dropped oldest: {}", &dropped.mint[..12]);
            }
        }

        // Add to queue
        seen.insert(candidate.mint.clone());
        queue.push_back(candidate.clone());
        
        info!("📥 Advisor: Queued {} from {:?} (score: {}, queue: {}/{})",
              &candidate.mint[..12], candidate.source, candidate.score, 
              queue.len(), self.max_size);
        
        true
    }

    /// Pop next candidate if available (with staleness check)
    pub async fn pop(&self) -> Option<AdvisorCandidate> {
        let mut queue = self.queue.write().await;
        
        // Keep popping until we find a fresh advisory or queue is empty
        loop {
            let candidate = queue.pop_front()?;
            let age_secs = candidate.queued_at.elapsed().as_secs();
            
            // Staleness threshold: 45 seconds max queue time
            // Rationale: Tokens can graduate to Raydium or get rugged in 30-60s
            const MAX_QUEUE_AGE_SECS: u64 = 45;
            
            if age_secs > MAX_QUEUE_AGE_SECS {
                warn!("⏰ STALE ADVISORY REJECTED: {} | queued {:.1}s ago (max: {}s) | source: {:?}",
                      &candidate.mint[..12], age_secs, MAX_QUEUE_AGE_SECS, candidate.source);
                // Continue to next advisory
                continue;
            }
            
            debug!("📤 Advisor: Dequeued {} (waited: {:.1}s, remaining: {})",
                   &candidate.mint[..12], age_secs, queue.len());
            
            return Some(candidate);
        }
    }

    /// Check if we can process advisor entry (throttling)
    pub async fn can_process(&self, max_rate_per_30s: u32, max_concurrent: u32) -> bool {
        // Check rate limit (30-second window)
        let last_entry = self.last_entry_time.read().await;
        if let Some(last) = *last_entry {
            if last.elapsed() < Duration::from_secs(30) {
                debug!("🕐 Advisor: Rate limited (last entry: {:.1}s ago)", last.elapsed().as_secs_f64());
                return false;
            }
        }

        // Check concurrent limit
        let count = *self.advisor_position_count.read().await;
        if count >= max_concurrent as usize {
            debug!("⚠️  Advisor: At max concurrent positions ({}/{})", count, max_concurrent);
            return false;
        }

        true
    }

    /// Mark that we processed an advisor entry
    pub async fn mark_processed(&self) {
        let mut last_entry = self.last_entry_time.write().await;
        *last_entry = Some(Instant::now());
        
        let mut count = self.advisor_position_count.write().await;
        *count += 1;
        
        info!("✅ Advisor: Entry processed (concurrent: {})", *count);
    }

    /// Mark that an advisor position was closed
    pub async fn mark_closed(&self, mint: &str) {
        let mut count = self.advisor_position_count.write().await;
        if *count > 0 {
            *count -= 1;
        }
        
        // Remove from seen set after a delay (allow re-entry after 5 minutes)
        let seen_clone = self.seen_mints.clone();
        let mint_clone = mint.to_string();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_secs(300)).await;
            seen_clone.write().await.remove(&mint_clone);
        });
        
        debug!("🔒 Advisor: Position closed {} (concurrent: {})", &mint[..12], *count);
    }

    /// Clear a specific mint from seen set (if entry aborted)
    pub async fn clear_seen(&self, mint: &str) {
        let mut seen = self.seen_mints.write().await;
        seen.remove(mint);
        debug!("🧹 Advisor: Cleared seen for {}", &mint[..12]);
    }

    /// Get queue stats
    pub async fn stats(&self) -> (usize, usize) {
        let queue_len = self.queue.read().await.len();
        let advisor_count = *self.advisor_position_count.read().await;
        (queue_len, advisor_count)
    }

    /// Check if queue has items
    pub async fn has_items(&self) -> bool {
        !self.queue.read().await.is_empty()
    }
}
