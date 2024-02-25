use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use listenfd::ListenFd;

use futures::stream::{Stream, StreamExt};

use network_interface::{NetworkInterface, NetworkInterfaceConfig};
use notify::Config;
use std::{
    collections::HashMap,
    error::Error,
    future::{self, IntoFuture},
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
use wake_runner::{
    os::users::watch_active_user_count,
    server::{app::create_app, config::init_config, server_state::ServerState},
};

use tokio_util::sync::CancellationToken;

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

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config = init_config().await;

    let (wake_process_count_setter, active_wake_process_count) = watch::channel(0);
    let tmp_active_wake = active_wake_process_count.clone();

    println!("Wakes {config:?}");
    let shutdown_signal = CancellationToken::new();
    let active_user_count = watch_active_user_count(shutdown_signal.clone()).await;

    tokio::spawn(shutdown_condition(
        active_user_count,
        tmp_active_wake,
        shutdown_signal.clone(),
    ));

    let mut listenfd = ListenFd::from_env();
    let listener = match listenfd.take_tcp_listener(0).unwrap() {
        Some(listener) => TcpListener::from_std(listener).unwrap(),
        None => TcpListener::bind("0.0.0.0:0").await.unwrap(),
    };

    let server_listen_port = listener.local_addr().unwrap().port();

    let app = create_app(wake_process_count_setter, active_wake_process_count, config);
    // let app = Router::new().with_state(Arc::new(config));
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
async fn discovery_server(shutdown_token: &CancellationToken, server_listen_port: u16) {
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
        };
    }
}

async fn get_wakes(state: State<Arc<ServerState>>) -> impl IntoResponse {
    Json(state.config.wakes.clone())
}
