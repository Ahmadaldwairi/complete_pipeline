use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub last_processed_slot: u64,
    pub last_updated: i64,
}

impl Checkpoint {
    /// Load checkpoint from file, returns None if file doesn't exist
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Option<Self>> {
        if !path.as_ref().exists() {
            info!("No checkpoint file found at {:?}, starting fresh", path.as_ref());
            return Ok(None);
        }

        let contents = fs::read_to_string(&path)
            .context("Failed to read checkpoint file")?;
        
        let checkpoint: Checkpoint = serde_json::from_str(&contents)
            .context("Failed to parse checkpoint file")?;
        
        info!("✅ Loaded checkpoint: slot {} (updated at {})", 
            checkpoint.last_processed_slot, checkpoint.last_updated);
        Ok(Some(checkpoint))
    }

    /// Save checkpoint to file
    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)
                .context("Failed to create checkpoint directory")?;
        }

        let contents = serde_json::to_string_pretty(self)
            .context("Failed to serialize checkpoint")?;
        
        // Write to temp file first, then rename (atomic operation)
        let temp_path = path.as_ref().with_extension("tmp");
        fs::write(&temp_path, contents)
            .context("Failed to write temp checkpoint file")?;
        
        fs::rename(&temp_path, &path)
            .context("Failed to rename checkpoint file")?;
        
        Ok(())
    }

    /// Create new checkpoint
    pub fn new(slot: u64) -> Self {
        Self {
            last_processed_slot: slot,
            last_updated: chrono::Utc::now().timestamp(),
        }
    }

    /// Update checkpoint with new slot
    pub fn update(&mut self, slot: u64) {
        self.last_processed_slot = slot;
        self.last_updated = chrono::Utc::now().timestamp();
    }

    /// Save checkpoint if enough slots have passed since last save
    pub fn save_if_needed<P: AsRef<Path>>(&self, path: P, current_slot: u64, interval: u64) -> Result<bool> {
        if current_slot >= self.last_processed_slot + interval {
            match self.save(&path) {
                Ok(_) => {
                    info!("💾 Checkpoint saved: slot {}", self.last_processed_slot);
                    Ok(true)
                }
                Err(e) => {
                    warn!("Failed to save checkpoint: {}", e);
                    Err(e)
                }
            }
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_checkpoint_save_and_load() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("checkpoint.json");

        // Create and save checkpoint
        let checkpoint = Checkpoint::new(12345);
        checkpoint.save(&path).unwrap();

        // Load checkpoint
        let loaded = Checkpoint::load(&path).unwrap().unwrap();
        assert_eq!(loaded.last_processed_slot, 12345);

        // Update and save again
        let mut updated = loaded;
        updated.update(67890);
        updated.save(&path).unwrap();

        // Load again
        let reloaded = Checkpoint::load(&path).unwrap().unwrap();
        assert_eq!(reloaded.last_processed_slot, 67890);
    }

    #[test]
    fn test_checkpoint_load_nonexistent() {
        let result = Checkpoint::load("nonexistent.json").unwrap();
        assert!(result.is_none());
    }
}
