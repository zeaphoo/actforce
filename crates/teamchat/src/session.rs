use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

use crate::models::gateway::GatewaySend;

const RING_SIZE: usize = 256;

#[derive(Debug, Clone)]
struct RingEntry {
    seq: u64,
    payload: GatewaySend,
}

pub struct SessionInner {
    pub session_id: String,
    pub subscribed_channels: Vec<String>,
    pub seq: u64,
    pub last_heartbeat: Instant,
    ring: Vec<RingEntry>,
    ring_pos: usize,
}

impl SessionInner {
    fn new(session_id: String, channels: Vec<String>) -> Self {
        Self {
            session_id,
            subscribed_channels: channels,
            seq: 0,
            last_heartbeat: Instant::now(),
            ring: Vec::with_capacity(RING_SIZE),
            ring_pos: 0,
        }
    }

    fn record(&mut self, payload: GatewaySend) {
        if self.ring.len() < RING_SIZE {
            self.ring.push(RingEntry { seq: self.seq, payload });
        } else {
            self.ring[self.ring_pos] = RingEntry { seq: self.seq, payload };
        }
        self.ring_pos = (self.ring_pos + 1) % RING_SIZE;
    }

    fn replay_since(&self, after_seq: u64) -> Vec<GatewaySend> {
        if self.ring.is_empty() {
            return Vec::new();
        }

        let mut result: Vec<_> = self
            .ring
            .iter()
            .filter(|entry| entry.seq > after_seq)
            .map(|entry| entry.payload.clone())
            .collect();

        result.sort_by_key(|p| p.s);
        result
    }
}

#[derive(Clone)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, SessionInner>>>,
    #[allow(dead_code)]
    timeout_secs: u64,
}

impl SessionManager {
    pub fn new(timeout_secs: u64) -> Self {
        let mgr = Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            timeout_secs,
        };

        let sessions = mgr.sessions.clone();
        let timeout = timeout_secs;
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
            loop {
                interval.tick().await;
                let now = Instant::now();
                sessions.write().await.retain(|_, s| {
                    now.duration_since(s.last_heartbeat).as_secs() < timeout + 30
                });
            }
        });

        mgr
    }

    pub async fn create(&self, channels: Vec<String>) -> String {
        let session_id = uuid::Uuid::new_v4().to_string();
        let inner = SessionInner::new(session_id.clone(), channels);
        self.sessions.write().await.insert(session_id.clone(), inner);
        session_id
    }

    pub async fn heartbeat(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(s) = sessions.get_mut(session_id) {
            s.last_heartbeat = Instant::now();
            true
        } else {
            false
        }
    }

    pub async fn get_channels(&self, session_id: &str) -> Option<Vec<String>> {
        self.sessions.read().await.get(session_id).map(|s| s.subscribed_channels.clone())
    }

    pub async fn dispatch(&self, session_id: &str, event: &str, data: serde_json::Value) -> Option<GatewaySend> {
        let mut sessions = self.sessions.write().await;
        let s = sessions.get_mut(session_id)?;
        s.seq += 1;
        let payload = GatewaySend::dispatch(event, data, s.seq);
        s.record(payload.clone());
        Some(payload)
    }

    pub async fn resume(&self, session_id: &str, seq: u64) -> Option<Vec<GatewaySend>> {
        let sessions = self.sessions.read().await;
        let s = sessions.get(session_id)?;
        Some(s.replay_since(seq))
    }

    pub async fn remove(&self, session_id: &str) {
        self.sessions.write().await.remove(session_id);
    }

    pub async fn exists(&self, session_id: &str) -> bool {
        self.sessions.read().await.contains_key(session_id)
    }
}
