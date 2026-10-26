use axum::extract::ws::{Message, WebSocket};
use futures_util::{SinkExt, StreamExt};
use tracing::{debug, warn};

use crate::models::gateway::{GatewayReceive, GatewaySend, SubscribePayload, ResumePayload};
use crate::server::TeamChatState;
use crate::store::TeamChatStore;

pub async fn handle_ws<S: TeamChatStore>(
    socket: WebSocket,
    state: TeamChatState<S>,
) {
    let (mut tx, mut rx) = socket.split();

    let session_id = state.session_mgr.create(Vec::new()).await;
    debug!("WS session created: {}", session_id);

    let hello = GatewaySend::hello(state.config.heartbeat_interval_ms);
    if let Ok(text) = serde_json::to_string(&hello) {
        if tx.send(Message::Text(text.into())).await.is_err() {
            return;
        }
    }

    let (shutdown_tx, mut shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);

    let heartbeat_interval = std::time::Duration::from_millis(state.config.heartbeat_interval_ms);
    let session_mgr = state.session_mgr.clone();
    let recv_session_id = session_id.clone();
    let send_session_id = session_id.clone();
    let send_session_mgr = state.session_mgr.clone();

    let send_task = tokio::spawn(async move {
        let mut heartbeat_timer = tokio::time::interval(heartbeat_interval);

        loop {
            tokio::select! {
                msg = rx.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            let parsed: GatewayReceive = match serde_json::from_str(&text) {
                                Ok(p) => p,
                                Err(_) => continue,
                            };

                            match parsed.op {
                                1 => {
                                    send_session_mgr.heartbeat(&send_session_id).await;
                                    let ack = GatewaySend::heartbeat_ack();
                                    if let Ok(s) = serde_json::to_string(&ack) {
                                        let _ = tx.send(Message::Text(s.into())).await;
                                    }
                                }
                                2 => {
                                    if let Ok(sub) = serde_json::from_value::<SubscribePayload>(parsed.d) {
                                        send_session_mgr.create(sub.channels.clone()).await;

                                        let ready = GatewaySend::dispatch("READY", serde_json::json!({"session_id": send_session_id}), 0);
                                        if let Ok(s) = serde_json::to_string(&ready) {
                                            let _ = tx.send(Message::Text(s.into())).await;
                                        }
                                    }
                                }
                                6 => {
                                    if let Ok(resume) = serde_json::from_value::<ResumePayload>(parsed.d) {
                                        if let Some(events) = send_session_mgr.resume(&resume.session_id, resume.seq).await {
                                            for event in &events {
                                                if let Ok(s) = serde_json::to_string(event) {
                                                    let _ = tx.send(Message::Text(s.into())).await;
                                                }
                                            }
                                            if !events.is_empty() {
                                                let last_s = events.last().unwrap().s.unwrap_or(0);
                                                let resumed = GatewaySend::dispatch("RESUMED", serde_json::json!({}), last_s);
                                                if let Ok(s) = serde_json::to_string(&resumed) {
                                                    let _ = tx.send(Message::Text(s.into())).await;
                                                }
                                            }
                                        } else {
                                            let invalid = GatewaySend::invalid_session();
                                            if let Ok(s) = serde_json::to_string(&invalid) {
                                                let _ = tx.send(Message::Text(s.into())).await;
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        Some(Ok(Message::Close(_))) | None => {
                            let _ = shutdown_tx.send(());
                            break;
                        }
                        _ => {}
                    }
                }
                _ = heartbeat_timer.tick() => {
                    // Server heartbeat timeout check
                }
                _ = shutdown_rx.recv() => {
                    break;
                }
            }
        }
    });

    let recv_task = tokio::spawn(async move {
        let mut interval = tokio::time::interval(heartbeat_interval);
        loop {
            interval.tick().await;
            if !session_mgr.heartbeat(&recv_session_id).await {
                warn!("Session {} expired", recv_session_id);
                break;
            }
        }
    });

    send_task.await.ok();
    recv_task.abort();
    state.session_mgr.remove(&session_id.clone()).await;
}
