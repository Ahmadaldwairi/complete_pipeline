/// Message Deduplication Module
/// 
/// Prevents duplicate processing of UDP messages by tracking (trade_id, msg_type) pairs
/// in an LRU cache. This eliminates issues like:
/// - Double Telegram notifications
/// - Duplicate decision processing
/// - Redundant database writes
///
/// Architecture:
/// - LRU cache with configurable capacity and TTL
/// - Key: (trade_id: u128, msg_type: u8)
/// - Automatic eviction of stale entries (>60s default)
/// - Thread-safe with Arc<Mutex<>>
///
/// Usage:
/// ```rust
/// let dedup = MessageDeduplicator::new(1000, Duration::from_secs(60));
/// if dedup.is_duplicate(trade_id, msg_type) {
///     debug!("Dropped duplicate message");
///     continue;
/// }
/// // Process message...
/// ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Unique identifier for a message: (trade_id, msg_type)
type MessageKey = (u128, u8);

/// Entry in the deduplication cache with timestamp for TTL
#[derive(Clone)]
struct CacheEntry {
    last_seen: Instant,
}

/// Message deduplicator using LRU cache pattern
///
/// Tracks recently seen (trade_id, msg_type) pairs to drop duplicates.
/// Automatically evicts entries older than TTL to prevent memory growth.
pub struct MessageDeduplicator {
    cache: Arc<Mutex<HashMap<MessageKey, CacheEntry>>>,
    max_capacity: usize,
    ttl: Duration,
    stats: Arc<Mutex<DeduplicationStats>>,
}

/// Statistics for monitoring deduplication effectiveness
#[derive(Debug, Default, Clone)]
pub struct DeduplicationStats {
    pub total_checked: u64,
    pub duplicates_dropped: u64,
    pub unique_messages: u64,
    pub cache_evictions: u64,
}

impl MessageDeduplicator {
    /// Create a new deduplicator
    ///
    /// # Arguments
    /// * `max_capacity` - Maximum number of messages to track (e.g., 1000)
    /// * `ttl` - Time-to-live for cache entries (e.g., 60 seconds)
    ///
    /// # Example
    /// ```rust
    /// let dedup = MessageDeduplicator::new(1000, Duration::from_secs(60));
    /// ```
    pub fn new(max_capacity: usize, ttl: Duration) -> Self {
        Self {
            cache: Arc::new(Mutex::new(HashMap::with_capacity(max_capacity))),
            max_capacity,
            ttl,
            stats: Arc::new(Mutex::new(DeduplicationStats::default())),
        }
    }

    /// Check if a message is a duplicate and mark it as seen if not
    ///
    /// Returns true if the message is a duplicate (should be dropped).
    /// Returns false if the message is unique (should be processed).
    ///
    /// # Arguments
    /// * `trade_id` - 128-bit trade ID from the message
    /// * `msg_type` - Message type byte (e.g., 26=TxConfirmed, 27=TxConfirmedContext)
    ///
    /// # Example
    /// ```rust
    /// if dedup.is_duplicate(trade_id, 26) {
    ///     debug!("Dropped duplicate TxConfirmed for trade {}", trade_id);
    ///     continue;
    /// }
    /// ```
    pub fn is_duplicate(&self, trade_id: u128, msg_type: u8) -> bool {
        let mut cache = self.cache.lock().unwrap();
        let mut stats = self.stats.lock().unwrap();
        
        stats.total_checked += 1;
        
        let key = (trade_id, msg_type);
        let now = Instant::now();
        
        // Check if message exists in cache and is still valid (within TTL)
        if let Some(entry) = cache.get(&key) {
            if now.duration_since(entry.last_seen) < self.ttl {
                // Duplicate found
                stats.duplicates_dropped += 1;
                return true;
            }
        }
        
        // Not a duplicate - add to cache
        cache.insert(key, CacheEntry { last_seen: now });
        stats.unique_messages += 1;
        
        // Evict stale entries if cache is getting large
        if cache.len() > self.max_capacity {
            self.evict_stale_entries(&mut cache, &mut stats, now);
        }
        
        false
    }

    /// Manually evict stale entries (called automatically when capacity exceeded)
    fn evict_stale_entries(
        &self,
        cache: &mut HashMap<MessageKey, CacheEntry>,
        stats: &mut DeduplicationStats,
        now: Instant,
    ) {
        let ttl = self.ttl;
        let initial_size = cache.len();
        
        cache.retain(|_, entry| {
            now.duration_since(entry.last_seen) < ttl
        });
        
        let evicted = initial_size - cache.len();
        stats.cache_evictions += evicted as u64;
    }

    /// Get current deduplication statistics
    pub fn stats(&self) -> DeduplicationStats {
        self.stats.lock().unwrap().clone()
    }

    /// Reset statistics (useful for testing or periodic reporting)
    pub fn reset_stats(&self) {
        let mut stats = self.stats.lock().unwrap();
        *stats = DeduplicationStats::default();
    }

    /// Get current cache size
    pub fn cache_size(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    /// Clear all cached entries (useful for testing)
    pub fn clear(&self) {
        self.cache.lock().unwrap().clear();
    }
}

impl DeduplicationStats {
    /// Calculate duplicate rate as percentage
    pub fn duplicate_rate(&self) -> f64 {
        if self.total_checked == 0 {
            0.0
        } else {
            (self.duplicates_dropped as f64 / self.total_checked as f64) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_deduplication() {
        let dedup = MessageDeduplicator::new(100, Duration::from_secs(60));
        
        let trade_id = 123456789u128;
        let msg_type = 26u8;
        
        // First occurrence should not be duplicate
        assert!(!dedup.is_duplicate(trade_id, msg_type));
        
        // Second occurrence should be duplicate
        assert!(dedup.is_duplicate(trade_id, msg_type));
        
        // Third occurrence should still be duplicate
        assert!(dedup.is_duplicate(trade_id, msg_type));
    }

    #[test]
    fn test_different_msg_types() {
        let dedup = MessageDeduplicator::new(100, Duration::from_secs(60));
        
        let trade_id = 123456789u128;
        
        // Same trade_id but different msg_type should not be duplicate
        assert!(!dedup.is_duplicate(trade_id, 26)); // TxConfirmed
        assert!(!dedup.is_duplicate(trade_id, 27)); // TxConfirmedContext
        
        // But second occurrence of same (trade_id, msg_type) should be duplicate
        assert!(dedup.is_duplicate(trade_id, 26));
        assert!(dedup.is_duplicate(trade_id, 27));
    }

    #[test]
    fn test_different_trade_ids() {
        let dedup = MessageDeduplicator::new(100, Duration::from_secs(60));
        
        let msg_type = 26u8;
        
        // Different trade_ids should not be duplicates
        assert!(!dedup.is_duplicate(111u128, msg_type));
        assert!(!dedup.is_duplicate(222u128, msg_type));
        assert!(!dedup.is_duplicate(333u128, msg_type));
    }

    #[test]
    fn test_ttl_expiration() {
        let dedup = MessageDeduplicator::new(100, Duration::from_millis(100));
        
        let trade_id = 123456789u128;
        let msg_type = 26u8;
        
        // First occurrence
        assert!(!dedup.is_duplicate(trade_id, msg_type));
        
        // Immediate check should be duplicate
        assert!(dedup.is_duplicate(trade_id, msg_type));
        
        // Wait for TTL to expire
        std::thread::sleep(Duration::from_millis(150));
        
        // Should no longer be duplicate after TTL expires
        assert!(!dedup.is_duplicate(trade_id, msg_type));
    }

    #[test]
    fn test_cache_eviction() {
        let dedup = MessageDeduplicator::new(10, Duration::from_secs(60));
        
        // Add 15 entries (exceeds capacity of 10)
        for i in 0..15 {
            assert!(!dedup.is_duplicate(i as u128, 26));
        }
        
        // Cache should have evicted stale entries
        assert!(dedup.cache_size() <= 15);
    }

    #[test]
    fn test_statistics() {
        let dedup = MessageDeduplicator::new(100, Duration::from_secs(60));
        
        // Process some messages
        dedup.is_duplicate(1u128, 26); // unique
        dedup.is_duplicate(1u128, 26); // duplicate
        dedup.is_duplicate(2u128, 26); // unique
        dedup.is_duplicate(1u128, 26); // duplicate
        
        let stats = dedup.stats();
        assert_eq!(stats.total_checked, 4);
        assert_eq!(stats.unique_messages, 2);
        assert_eq!(stats.duplicates_dropped, 2);
        assert_eq!(stats.duplicate_rate(), 50.0);
    }

    #[test]
    fn test_clear() {
        let dedup = MessageDeduplicator::new(100, Duration::from_secs(60));
        
        dedup.is_duplicate(1u128, 26);
        dedup.is_duplicate(2u128, 26);
        assert_eq!(dedup.cache_size(), 2);
        
        dedup.clear();
        assert_eq!(dedup.cache_size(), 0);
        
        // After clear, previously seen messages should not be duplicates
        assert!(!dedup.is_duplicate(1u128, 26));
    }
}
