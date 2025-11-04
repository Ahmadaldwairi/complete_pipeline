//! 🗺️ Position Mapping - Track Bonding Curve ↔ Mint Relationships
//!
//! When Brain enters a position, it needs to monitor the bonding curve account
//! for price updates. This module maintains the bidirectional mapping between
//! bonding curve PDAs and mint addresses.

use dashmap::DashMap;
use solana_sdk::pubkey::Pubkey;
use std::sync::Arc;

/// Tracks bidirectional mapping between bonding curves and mints
pub struct PositionMapping {
    /// bonding_curve_pda → mint
    curve_to_mint: Arc<DashMap<Pubkey, Pubkey>>,
    /// mint → bonding_curve_pda
    mint_to_curve: Arc<DashMap<Pubkey, Pubkey>>,
}

impl PositionMapping {
    /// Create new position mapping
    pub fn new() -> Self {
        Self {
            curve_to_mint: Arc::new(DashMap::new()),
            mint_to_curve: Arc::new(DashMap::new()),
        }
    }

    /// Add position mapping (called when entering position)
    pub fn add_position(&self, mint: Pubkey, bonding_curve: Pubkey) {
        self.curve_to_mint.insert(bonding_curve, mint);
        self.mint_to_curve.insert(mint, bonding_curve);
        log::debug!(
            "📍 Mapped position: mint {} → curve {}",
            &mint.to_string()[..12],
            &bonding_curve.to_string()[..12]
        );
    }

    /// Remove position mapping (called when exiting position)
    pub fn remove_position(&self, mint: &Pubkey) {
        if let Some((_, bonding_curve)) = self.mint_to_curve.remove(mint) {
            self.curve_to_mint.remove(&bonding_curve);
            log::debug!(
                "🗑️  Unmapped position: mint {}",
                &mint.to_string()[..12]
            );
        }
    }

    /// Get mint from bonding curve PDA
    pub fn get_mint_from_curve(&self, bonding_curve: &Pubkey) -> Option<Pubkey> {
        self.curve_to_mint.get(bonding_curve).map(|entry| *entry.value())
    }

    /// Get bonding curve PDA from mint
    pub fn get_curve_from_mint(&self, mint: &Pubkey) -> Option<Pubkey> {
        self.mint_to_curve.get(mint).map(|entry| *entry.value())
    }

    /// Check if mint is currently tracked
    pub fn is_tracking(&self, mint: &Pubkey) -> bool {
        self.mint_to_curve.contains_key(mint)
    }

    /// Get count of tracked positions
    pub fn count(&self) -> usize {
        self.mint_to_curve.len()
    }
}

impl Default for PositionMapping {
    fn default() -> Self {
        Self::new()
    }
}
