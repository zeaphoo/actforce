use std::sync::Arc;

use axum::{
    extract::State,
    routing::{delete, get, patch, post},
    Router,
};
use tower_http::cors::{Any, CorsLayer};

use crate::config::Config;
use crate::hub::ChannelHub;
use crate::session::SessionManager;
use crate::store::TeamChatStore;

pub struct TeamChatState<S: TeamChatStore> {
    pub store: Arc<S>,
    pub hub: Arc<ChannelHub>,
    pub session_mgr: Arc<SessionManager>,
    pub config: Arc<Config>,
}

impl<S: TeamChatStore> Clone for TeamChatState<S> {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            hub: self.hub.clone(),
            session_mgr: self.session_mgr.clone(),
            config: self.config.clone(),
        }
    }
}

pub fn build_router<S: TeamChatStore>(store: Arc<S>, config: Arc<Config>) -> Router {
    let state = TeamChatState {
        store,
        hub: Arc::new(ChannelHub::new()),
        session_mgr: Arc::new(SessionManager::new(config.session_timeout_secs)),
        config,
    };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/channels", get(crate::routes::channels::list::<S>))
        .route("/channels", post(crate::routes::channels::create::<S>))
        .route("/channels/{channel_id}", get(crate::routes::channels::get::<S>))
        .route("/channels/{channel_id}", patch(crate::routes::channels::update::<S>))
        .route("/channels/{channel_id}", delete(crate::routes::channels::delete::<S>))
        .route("/channels/{channel_id}/messages", get(crate::routes::messages::list::<S>))
        .route("/channels/{channel_id}/messages", post(crate::routes::messages::create::<S>))
        .route("/channels/{channel_id}/messages/{message_id}", get(crate::routes::messages::get::<S>))
        .route("/channels/{channel_id}/messages/{message_id}", patch(crate::routes::messages::update::<S>))
        .route("/channels/{channel_id}/messages/{message_id}", delete(crate::routes::messages::delete::<S>))
        .route("/ws", get(handle_ws_wrapper::<S>))
        .layer(cors)
        .with_state(state)
}

async fn handle_ws_wrapper<S: TeamChatStore>(
    ws: axum::extract::WebSocketUpgrade,
    State(state): State<TeamChatState<S>>,
) -> axum::response::Response {
    ws.on_upgrade(|socket| crate::ws::handle_ws(socket, state))
}
