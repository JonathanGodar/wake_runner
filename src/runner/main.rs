use axum::{
    extract::State,
    handler::Handler,
    http::{Response, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use listenfd::ListenFd;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    error::Error,
    process::Stdio,
    sync::{Arc, Mutex},
};
use tokio::{fs, net::TcpListener, process::Command};
use tokio::{io::AsyncReadExt, process::Child};
use uuid::Uuid;

mod active_user_count;

type WakeProcessMap = HashMap<String, WakeProcess>;

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    wakes: Vec<Wake>, // commands: HashMap<String, Table>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Wake {
    name: String,
    command: String,

    #[serde(default)]
    multiple_instances: bool,
    working_directory: Option<String>,

    #[serde(default)]
    arguments: Vec<String>,
}

struct AppState {
    config: Config,
    wake_processes: Mutex<WakeProcessMap>,
}

struct WakeProcess {
    id: String,
    name: String,
    process: Child,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config_str = fs::read_to_string("runner_config.toml").await.unwrap();
    let config: Config = toml::from_str(&config_str).unwrap();

    let wake_processes: HashMap<String, WakeProcess> = HashMap::new();

    let app_state = AppState {
        config,
        wake_processes: wake_processes.into(),
    };

    let app = Router::new()
        .route("/wakes", get(get_wakes))
        .route("/wake_run", post(wake_run))
        .with_state(Arc::new(app_state));

    let mut listenfd = ListenFd::from_env();
    let listener = match listenfd.take_tcp_listener(0).unwrap() {
        Some(listener) => TcpListener::from_std(listener).unwrap(),
        None => TcpListener::bind("0.0.0.0:3000").await.unwrap(),
    };

    // let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on: {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
    Ok(())
}

// async fn get_output_stream() ->  {}

#[derive(Debug, Deserialize)]
struct StartWakeRunBody {
    name: String,
}

async fn get_wakes(state: State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.config.wakes.clone())
}

async fn wake_run(
    state: State<Arc<AppState>>,
    Json(payload): Json<StartWakeRunBody>,
) -> impl IntoResponse {
    let maybe_wake = state
        .config
        .wakes
        .iter()
        .find(|wakerun| wakerun.name == payload.name);

    match maybe_wake {
        Some(wake) => {
            let mut command = construct_wake_command(&wake);
            let process = command.spawn().unwrap();

            let id = Uuid::new_v4().to_string();

            let wake_process = WakeProcess {
                name: wake.name.clone(),
                process,
                id: id.clone(),
            };

            let mut map = state.wake_processes.lock().unwrap();
            map.insert(id.clone(), wake_process);

            (StatusCode::OK, Json(json!({ "id": id })))
        }
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))),
    }
}

fn construct_wake_command(wake: &Wake) -> Command {
    let mut command = Command::new(&wake.name);
    command.stdin(Stdio::null());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    command.args(&wake.arguments);

    if let Some(dir) = &wake.working_directory {
        command.current_dir(dir);
    }

    command
}
