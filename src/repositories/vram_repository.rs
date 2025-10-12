use serde_json::Value;
use std::str::FromStr;
use tokio::process::Command;
use tracing::error;

pub struct VramRepository;

#[derive(Default)]
struct Memory {
    total_mb: u64,
    used_mb: u64,
}

impl VramRepository {
    pub fn new() -> VramRepository {
        Self
    }

    pub async fn get_total_memory(&self) -> u64 {
        self.get_memory().await.total_mb
    }

    pub async fn get_used_memory(&self) -> u64 {
        self.get_memory().await.used_mb
    }

    pub async fn get_free_memory(&self) -> u64 {
        self.get_total_memory().await - self.get_used_memory().await
    }

    async fn get_memory(&self) -> Memory {
        // Execute the CLI tool.
        let output = Command::new("rocm-smi")
            .arg("--showmeminfo")
            .arg("vram")
            .arg("--json")
            .output()
            .await
            .expect("failed to execute rocm-smi");

        if !output.status.success() {
            // If the tool failed we treat it as no free memory (conservative).
            error!("rocm-smi returned a non‑zero exit code");
            return Memory::default();
        }

        // Parse the JSON payload.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let v: Value = match serde_json::from_str(&stdout) {
            Ok(v) => v,
            Err(e) => {
                error!("Failed to parse rocm‑smi JSON output: {}", e);
                return Memory::default();
            }
        };

        // The JSON has a top‑level key like "card0". Grab the first object.
        let card = match v.as_object().and_then(|obj| obj.values().next()) {
            Some(c) => c,
            None => {
                error!("Unexpected rocm‑smi JSON structure");
                return Memory::default();
            }
        };

        // Extract the two fields we need.
        let total_str = card
            .get("VRAM Total Memory (B)")
            .and_then(|v| v.as_str())
            .unwrap_or("0");
        let used_str = card
            .get("VRAM Total Used Memory (B)")
            .and_then(|v| v.as_str())
            .unwrap_or("0");

        let total = u64::from_str(total_str).unwrap_or(0);
        let used = u64::from_str(used_str).unwrap_or(0);

        Memory {
            total_mb: total.div_ceil(1_000_000),
            used_mb: used.div_ceil(1_000_000),
        }
    }
}
