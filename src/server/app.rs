use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{response::IntoResponse, routing::get, Json, Router};
use tokio::sync::watch;

use super::{
    server_state::ServerState,
    wake::{router::create_router, WakeProcess},
};

pub fn create_app(
    wake_process_count_setter: watch::Sender<usize>,
    active_wake_process_count: watch::Receiver<usize>,
    config: super::config::Config,
) -> Router<()> {
    let wake_processes: HashMap<String, WakeProcess> = HashMap::new();

    let app_state = Arc::new(ServerState {
        config,
        wake_processes: wake_processes.into(),
        active_wake_process_count,
        active_wake_process_count_setter: Mutex::new(wake_process_count_setter),
    });

    let app = Router::new()
        .route("/ping", get(ping))
        .nest("/wake", create_router(app_state.clone()))
        // .route("/wakes", get(get_wakes))
        // .route("/ping", get(ping))
        // .route("/wake_run", post(wake_run))
        .with_state(app_state);

    app
}

async fn ping() -> impl IntoResponse {
    Json("pong")
}
