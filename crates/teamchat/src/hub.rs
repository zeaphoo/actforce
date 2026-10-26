use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct ChannelHub {
    channels: Arc<std::sync::RwLock<HashMap<String, broadcast::Sender<String>>>>,
}

impl ChannelHub {
    pub fn new() -> Self {
        Self {
            channels: Arc::new(std::sync::RwLock::new(HashMap::new())),
        }
    }

    pub fn get_or_create_sender(&self, channel_id: &str) -> broadcast::Sender<String> {
        let mut chs = self.channels.write().unwrap();
        chs.entry(channel_id.to_string())
            .or_insert_with(|| {
                let (tx, _) = broadcast::channel(256);
                tx
            })
            .clone()
    }

    pub fn broadcast(&self, channel_id: &str, msg: &str) {
        let chs = self.channels.read().unwrap();
        if let Some(tx) = chs.get(channel_id) {
            let _ = tx.send(msg.to_string());
        }
    }
}
