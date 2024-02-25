use std::{process::Stdio, sync::Arc};

use axum::{extract::State, http::StatusCode, response::IntoResponse, routing::post, Json, Router};
use serde_json::json;
use tokio::process::{Child, Command};
use uuid::Uuid;

use crate::server::{server_state::ServerState, wake::WakeProcess};

use super::{start_wake_body_dto::StartWakeBody, Wake};

pub fn create_router(state: Arc<ServerState>) -> Router<Arc<ServerState>> {
    Router::new().route("/", post(wake_run)).with_state(state)
}

pub async fn wake_run(
    state: State<Arc<ServerState>>,
    Json(payload): Json<StartWakeBody>,
) -> impl IntoResponse {
    let maybe_wake = state
        .config
        .wakes
        .iter()
        .find(|wakerun| wakerun.name == payload.name);

    match maybe_wake {
        Some(wake) => {
            let mut command = construct_wake_command(&wake);
            let id = Uuid::new_v4().to_string();

            println!("Starting wake: {:?} with id: {:?}", wake.name, id);
            let mut process = command.spawn().unwrap();

            {
                let lock = state.active_wake_process_count_setter.lock().unwrap();
                lock.send_modify(|val| *val += 1);
            }

            let output = process.stdout.take().unwrap();

            let wake_process = WakeProcess {
                name: wake.name.clone(),
                id: id.clone(),
                std_out: output,
            };

            {
                let mut map = state.wake_processes.lock().unwrap();
                map.insert(id.clone(), wake_process);
            }

            tokio::spawn(handle_wake_process(process, id.clone(), state.0.clone()));
            (StatusCode::OK, Json(json!({ "id": id })))
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "No wake with that id"})),
        ),
    }
}

async fn handle_wake_process(
    mut wake_process: Child,
    wake_process_id: String,
    app_state: Arc<ServerState>,
) {
    wake_process.wait().await.unwrap();

    {
        let mut map = app_state.wake_processes.lock().unwrap();
        map.remove(&wake_process_id);
        dbg!(&map);
    }

    {
        let lock = app_state.active_wake_process_count_setter.lock().unwrap();
        lock.send_modify(|val| *val -= 1);
    }

    println!("Wake {:?} exited", wake_process_id)
}

fn construct_wake_command(wake: &Wake) -> Command {
    println!("Constructing command: {wake:?}");
    let mut command = Command::new(&wake.command);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    command.args(&wake.arguments);

    if let Some(dir) = &wake.working_directory {
        command.current_dir(dir);
    }

    command
}
