use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

#[derive(Debug, Deserialize)]
pub struct BridgeMessage {
    pub id: String,
    pub command: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct BridgeResponse {
    pub id: String,
    pub result: Result<serde_json::Value, String>,
}

#[derive(Debug, Serialize)]
pub struct ProgressEvent {
    pub plugin_id: String,
    pub task_id: String,
    pub data: serde_json::Value,
}

pub struct BridgeHandler {
    pub tx: mpsc::Sender<BridgeMessage>,
}

impl BridgeHandler {
    pub fn new() -> (Self, mpsc::Receiver<BridgeMessage>) {
        let (tx, rx) = mpsc::channel(100);
        (Self { tx }, rx)
    }

    pub async fn handle_message(&self, msg: BridgeMessage) -> BridgeResponse {
        // Placeholder - will be implemented with actual Tauri commands
        BridgeResponse {
            id: msg.id,
            result: Err("Not implemented".to_string()),
        }
    }
}
