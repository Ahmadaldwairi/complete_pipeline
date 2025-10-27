use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub last_processed_slot: u64,
    pub last_updated: i64,
}

impl Checkpoint {
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Option<Self>> {
        if !path.as_ref().exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(&path)
            .context("Failed to read checkpoint file")?;
        
        let checkpoint: Checkpoint = serde_json::from_str(&contents)
            .context("Failed to parse checkpoint file")?;
        
        info!("Loaded checkpoint: slot {}", checkpoint.last_processed_slot);
        Ok(Some(checkpoint))
    }

    pub fn save<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)
                .context("Failed to create checkpoint directory")?;
        }

        let contents = serde_json::to_string_pretty(self)
            .context("Failed to serialize checkpoint")?;
        
        fs::write(&path, contents)
            .context("Failed to write checkpoint file")?;
        
        Ok(())
    }

    pub fn new(slot: u64) -> Self {
        Self {
            last_processed_slot: slot,
            last_updated: chrono::Utc::now().timestamp(),
        }
    }

    pub fn update(&mut self, slot: u64) {
        self.last_processed_slot = slot;
        self.last_updated = chrono::Utc::now().timestamp();
    }
}
