/// Mint Reservation System
/// 
/// Prevents duplicate BUY decisions by maintaining time-based leases on mints.
/// When Brain sends a BUY decision, it reserves the mint for a TTL period (default 30s).
/// Any subsequent BUY attempts during the lease period are rejected.
/// 
/// This provides an additional safety layer on top of the state machine.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Represents a reserved mint with expiration
#[derive(Debug, Clone)]
pub struct MintReservation {
    pub mint: String,
    pub trade_id: String,
    pub reserved_at: Instant,
    pub ttl: Duration,
}

impl MintReservation {
    /// Check if reservation has expired
    pub fn is_expired(&self) -> bool {
        self.reserved_at.elapsed() > self.ttl
    }
    
    /// Get age in seconds
    pub fn age_secs(&self) -> u64 {
        self.reserved_at.elapsed().as_secs()
    }
}

/// Manages mint reservations to prevent duplicate BUY decisions
pub struct MintReservationManager {
    reservations: HashMap<String, MintReservation>,
    default_ttl: Duration,
}

impl MintReservationManager {
    /// Create new manager with default TTL (seconds)
    pub fn new(default_ttl_secs: u64) -> Self {
        Self {
            reservations: HashMap::new(),
            default_ttl: Duration::from_secs(default_ttl_secs),
        }
    }
    
    /// Check if mint is currently reserved
    pub fn is_reserved(&self, mint: &str) -> bool {
        if let Some(reservation) = self.reservations.get(mint) {
            !reservation.is_expired()
        } else {
            false
        }
    }
    
    /// Get reservation details if it exists and is not expired
    pub fn get_reservation(&self, mint: &str) -> Option<&MintReservation> {
        self.reservations.get(mint).filter(|r| !r.is_expired())
    }
    
    /// Reserve a mint with default TTL
    /// Returns true if reservation succeeded, false if already reserved
    pub fn reserve(&mut self, mint: String, trade_id: String) -> bool {
        self.reserve_with_ttl(mint, trade_id, self.default_ttl)
    }
    
    /// Reserve a mint with custom TTL
    /// Returns true if reservation succeeded, false if already reserved
    pub fn reserve_with_ttl(&mut self, mint: String, trade_id: String, ttl: Duration) -> bool {
        // Check if already reserved
        if self.is_reserved(&mint) {
            return false;
        }
        
        // Create new reservation
        let reservation = MintReservation {
            mint: mint.clone(),
            trade_id,
            reserved_at: Instant::now(),
            ttl,
        };
        
        self.reservations.insert(mint, reservation);
        true
    }
    
    /// Manually release a reservation
    pub fn release(&mut self, mint: &str) {
        self.reservations.remove(mint);
    }
    
    /// Clean up expired reservations
    /// Returns number of reservations cleaned up
    pub fn cleanup_expired(&mut self) -> usize {
        let before_count = self.reservations.len();
        self.reservations.retain(|_, reservation| !reservation.is_expired());
        let after_count = self.reservations.len();
        before_count - after_count
    }
    
    /// Get number of active (non-expired) reservations
    pub fn active_count(&self) -> usize {
        self.reservations.values().filter(|r| !r.is_expired()).count()
    }
    
    /// Get total reservations (including expired)
    pub fn total_count(&self) -> usize {
        self.reservations.len()
    }
    
    /// Get stats for monitoring
    pub fn get_stats(&self) -> ReservationStats {
        let total = self.reservations.len();
        let active = self.active_count();
        let expired = total - active;
        
        ReservationStats {
            total,
            active,
            expired,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReservationStats {
    pub total: usize,
    pub active: usize,
    pub expired: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    
    #[test]
    fn test_reservation_basic() {
        let mut manager = MintReservationManager::new(30);
        let mint = "test_mint_123".to_string();
        let trade_id = "trade_001".to_string();
        
        // Should not be reserved initially
        assert!(!manager.is_reserved(&mint));
        
        // Reserve should succeed
        assert!(manager.reserve(mint.clone(), trade_id.clone()));
        
        // Should now be reserved
        assert!(manager.is_reserved(&mint));
        
        // Duplicate reserve should fail
        assert!(!manager.reserve(mint.clone(), "trade_002".to_string()));
    }
    
    #[test]
    fn test_reservation_expiry() {
        let mut manager = MintReservationManager::new(1); // 1 second TTL
        let mint = "test_mint_expiry".to_string();
        
        manager.reserve(mint.clone(), "trade_001".to_string());
        assert!(manager.is_reserved(&mint));
        
        // Wait for expiry
        thread::sleep(Duration::from_millis(1100));
        
        // Should no longer be reserved
        assert!(!manager.is_reserved(&mint));
        
        // Should be able to reserve again
        assert!(manager.reserve(mint.clone(), "trade_002".to_string()));
    }
    
    #[test]
    fn test_cleanup() {
        let mut manager = MintReservationManager::new(1);
        
        manager.reserve("mint1".to_string(), "trade1".to_string());
        manager.reserve("mint2".to_string(), "trade2".to_string());
        manager.reserve("mint3".to_string(), "trade3".to_string());
        
        assert_eq!(manager.total_count(), 3);
        assert_eq!(manager.active_count(), 3);
        
        // Wait for expiry
        thread::sleep(Duration::from_millis(1100));
        
        assert_eq!(manager.active_count(), 0);
        
        // Cleanup should remove all expired
        let cleaned = manager.cleanup_expired();
        assert_eq!(cleaned, 3);
        assert_eq!(manager.total_count(), 0);
    }
    
    #[test]
    fn test_release() {
        let mut manager = MintReservationManager::new(30);
        let mint = "test_release".to_string();
        
        manager.reserve(mint.clone(), "trade_001".to_string());
        assert!(manager.is_reserved(&mint));
        
        manager.release(&mint);
        assert!(!manager.is_reserved(&mint));
        
        // Should be able to reserve again
        assert!(manager.reserve(mint.clone(), "trade_002".to_string()));
    }
}
