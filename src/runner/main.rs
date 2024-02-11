use active_user_count::watch_active_user_count;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use listenfd::ListenFd;

use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use serde::{Deserialize, Serialize};
use serde_json::json;
use start_wake_body::StartWakeBody;
use std::{
    collections::HashMap,
    error::Error,
    future::IntoFuture,
    net::{Ipv4Addr, SocketAddrV4},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
    vec,
};
use system_shutdown::shutdown;
use tokio::process::Child;
use tokio::{
    fs,
    net::TcpListener,
    process::{ChildStdout, Command},
    select,
    sync::watch,
};

use tokio_util::sync::CancellationToken;
use uuid::Uuid;

mod active_user_count;
mod start_wake_body;

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

async fn shutdown_condition(
    mut user_count: watch::Receiver<usize>,
    mut wake_process_count: watch::Receiver<usize>,
    shutdown_token: CancellationToken,
) {
    loop {
        // Wait for the user and wake count to be 0
        let mut users_zero = *user_count.borrow() == 0;
        let mut wakes_zero = *wake_process_count.borrow() == 0;
        println!("Waiting for users, wakes to be 0");
        while !users_zero || !wakes_zero {
            select! {
                result = user_count.changed() => {
                    result.unwrap();
                    users_zero = *user_count.borrow() == 0;
                },
                result = wake_process_count.changed() => {
                    result.unwrap();
                    wakes_zero = *wake_process_count.borrow() == 0;
                }
            }
        }

        // Wait for 60 seconds and if there has been no changes to the counts, we should shutdown
        println!("No users, no wakes, waiting for shutdown");
        select! {
            _ = user_count.changed() => {},
            _ = wake_process_count.changed() => {},
            _ = tokio::time::sleep(Duration::from_secs(60)) => {
                break;
            }
        }
        println!("Shutdown aborted")
    }

    println!("Shutting down!");
    shutdown_token.cancel();
}

struct AppState {
    config: Config,
    wake_processes: Mutex<WakeProcessMap>,
    active_wake_process_count_setter: Mutex<watch::Sender<usize>>,
    active_wake_process_count: watch::Receiver<usize>,
}

#[derive(Debug)]
struct WakeProcess {
    id: String,
    name: String,
    std_out: ChildStdout,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config_str = fs::read_to_string("runner_config.toml").await.unwrap();
    let config: Config = toml::from_str(&config_str).unwrap();
    let (wake_process_count_setter, active_wake_process_count) = watch::channel(0);
    let tmp_active_wake = active_wake_process_count.clone();
    let wake_processes: HashMap<String, WakeProcess> = HashMap::new();

    println!("Wakes {config:?}");

    let shutdown_signal = CancellationToken::new();
    let active_user_count = watch_active_user_count(shutdown_signal.clone()).await;
    tokio::spawn(shutdown_condition(
        active_user_count,
        tmp_active_wake,
        shutdown_signal.clone(),
    ));

    let app_state = Arc::new(AppState {
        config,
        wake_processes: wake_processes.into(),
        active_wake_process_count,
        active_wake_process_count_setter: Mutex::new(wake_process_count_setter),
    });

    let app = Router::new()
        .route("/wakes", get(get_wakes))
        .route("/ping", get(ping))
        .route("/wake_run", post(wake_run))
        .with_state(app_state);

    let mut listenfd = ListenFd::from_env();
    let listener = match listenfd.take_tcp_listener(0).unwrap() {
        Some(listener) => TcpListener::from_std(listener).unwrap(),
        None => TcpListener::bind("0.0.0.0:0").await.unwrap(),
    };

    let server_listen_port = listener.local_addr().unwrap().port();

    println!("Listening on: {}", listener.local_addr().unwrap());
    let mut should_shutoff = false;
    select! {
        res = axum::serve(listener, app).into_future() => {res.unwrap()},
        _ = shutdown_signal.cancelled() => {
           should_shutoff = true;
        },
        _ = discovery_server(&shutdown_signal, server_listen_port) => {
            println!("discovery_server shut down");
        },
    }

    if should_shutoff {
        shutdown().unwrap();
    }

    Ok(())
}

pub fn get_addresses(
    include_local: bool,
) -> Result<Vec<network_interface::V4IfAddr>, Box<dyn Error>> {
    let mut networks = vec![];
    let ifs = NetworkInterface::show()?;
    for nif in ifs {
        for addr in &nif.addr {
            match addr {
                network_interface::Addr::V4(v4_addr) => {
                    if !include_local && v4_addr.ip.is_loopback() {
                        continue;
                    }
                    networks.push(v4_addr.clone());
                }
                _ => {}
            }
        }
        println!("{:?}", nif.addr);
    }

    Ok(networks)
}

const UDP_DISCOVERY_PORT: u16 = 23032;
async fn discovery_server(
    shutdown_token: &CancellationToken,
    server_listen_port: u16,
    // server_listen_address: Vec<SocketAddrV4>,
) {
    println!("Starting discovery server on {}", UDP_DISCOVERY_PORT);
    let udp_socket =
        tokio::net::UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, UDP_DISCOVERY_PORT))
            .await
            .unwrap();

    let response_bytes = server_listen_port.to_le_bytes();
    println!("Response bytes: {:?}", response_bytes);

    let mut buf = Vec::with_capacity(1024);
    loop {
        println!(
            "Listening on discovery on {:?}",
            udp_socket.local_addr().unwrap()
        );
        if let Ok((n_bytes, from)) = udp_socket.recv_buf_from(&mut buf).await {
            println!("Got {}, from {:?}", n_bytes, from);
            udp_socket.send_to(&response_bytes, from).await.unwrap();
            println!("Local address {:?}", udp_socket.local_addr());
            // udp_socket
            //     .send_to("HEJSANNNN".as_bytes(), from)
            //     .await
            //     .unwrap();
        };

        // select! {
        // TODO Handle Result::Error
        // _ =  shutdown_token.cancelled() => {
        //     break;
        // }
        // }
    }
}

async fn get_wakes(state: State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.config.wakes.clone())
}

async fn ping() -> impl IntoResponse {
    Json("pong")
}

async fn wake_run(
    state: State<Arc<AppState>>,
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
        None => (StatusCode::NOT_FOUND, Json(json!({"error": "Not found"}))),
    }
}

async fn handle_wake_process(
    mut wake_process: Child,
    wake_process_id: String,
    app_state: Arc<AppState>,
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
