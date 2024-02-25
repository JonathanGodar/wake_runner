use std::{
    error::Error,
    net::{SocketAddr, SocketAddrV4},
    time::Duration,
};

use mac_address::MacAddress;
use serde::{Deserialize, Serialize};
use tokio::{net::UdpSocket, select, task::JoinSet};
use tokio_util::sync::CancellationToken;
use wake_runner::net::{interfaces::get_broadcastable_v4_interfaces, wake_on_lan};

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    mac_address: String,
}

#[derive(Serialize)]
struct WakeRunBody {
    name: String,
}

const UDP_DISCOVERY_PORT: u16 = 23032;

async fn send_wake(mac_address: MacAddress, wake: &str) -> Option<()> {
    let address_v4 = {
        let address = awake_wake_runner(mac_address, Duration::from_secs(40)).await?;
        let a = match address {
            SocketAddr::V4(v4) => v4,
            SocketAddr::V6(_) => return None,
        };
        a
    };

    let uri = runner_uri_from_socket(address_v4);
    send_wake_to_uri(&uri, wake).await;
    Some(())
}

async fn send_wake_to_uri(uri: &str, wake: &str) {
    let http = reqwest::Client::new();
    let result = http
        .post(format!("{uri}/wake"))
        .json(&WakeRunBody {
            name: wake.to_string(),
        })
        .send()
        .await;

    dbg!(result);
}

// Should be cancel safe? No tokio::spawn so no memory leaks?
async fn ping_for_wake_runner_address(
    mac_address: MacAddress,
    timeout_duration: Duration,
) -> Option<SocketAddr> {
    let interfaces = get_broadcastable_v4_interfaces().unwrap();
    let mut handles = JoinSet::new();

    let cancel = CancellationToken::new();

    for interface in interfaces {
        let cancel = cancel.clone();
        let src_addr = interface.ip;
        let broadcast_addr = interface.broadcast.unwrap();

        let socket = UdpSocket::bind(SocketAddrV4::new(src_addr, 0))
            .await
            .unwrap();

        println!("Listening on: {:?}", socket.local_addr());
        socket.set_broadcast(true).unwrap();
        socket
            .send_to(
                &mac_address.bytes(),
                SocketAddrV4::new(broadcast_addr, UDP_DISCOVERY_PORT),
            )
            .await
            .unwrap();

        handles.spawn(async move {
            let mut read = Vec::with_capacity(2048);
            select! {
                _ = cancel.cancelled() => {}
                Ok((len, remote_sock)) = socket.recv_buf_from(&mut read) => {
                    println!("Recieved {:?}", &read[..len]);
                    if len != 2 {
                        return None;
                    }
                    let port = u16::from_le_bytes([read[0], read[1]]);
                    return Some(SocketAddr::new(remote_sock.ip(), port));
                }
            }

            None
        });
    }

    let udp_response = async {
        while let Some(res) = handles.join_next().await {
            if let Ok(Some(addr)) = res {
                return Some(addr);
            }
        }
        None
    };

    let maybe_socket = select! {
        _ = tokio::time::sleep(timeout_duration) => None,
        resp = udp_response => resp
    };

    maybe_socket
}

fn runner_uri_from_socket(socket: SocketAddrV4) -> String {
    return format!("http://{:?}:{:?}", socket.ip(), socket.port());
}

async fn ping_wake_runner(uri: String) -> Option<()> {
    let http = reqwest::Client::new();
    let request = http.get(format!("{uri}/ping")).send().await;
    match request {
        Ok(resp) => {
            let json: String = resp.json().await.ok()?;
            if json.to_string() != "pong" {
                return None;
            } else {
                return Some(());
            }
        }
        Err(err) => println!("{:?}", err),
    }

    return None;
}

async fn exponential_backoff_wakonlan(
    mac_address: MacAddress,
    cancel: CancellationToken,
    max_reqs: usize,
) {
    let mut sleep_time = Duration::from_millis(25);
    for _ in 0..max_reqs {
        select! {
            _ = cancel.cancelled() => {
                break;
            }
            _ = tokio::time::sleep(sleep_time) => {
                wake_on_lan::send(mac_address.bytes()).await.unwrap();
                println!("Sent wake-on-lan request");
            }
        }

        sleep_time *= 2;
    }
}

async fn awake_wake_runner(
    mac_address: MacAddress,
    timeout_duration: Duration,
) -> Option<SocketAddr> {
    let timeout = CancellationToken::new();

    // Fix this so we can drop
    let _exponential_backoff_handle = {
        let timeout = timeout.clone();
        let mac_address = mac_address.clone();
        tokio::spawn(exponential_backoff_wakonlan(mac_address, timeout, 10))
    };

    let find_wake_runner_address = async {
        loop {
            if let Some(addr) =
                ping_for_wake_runner_address(mac_address, Duration::from_millis(300)).await
            {
                break addr;
            }
        }
    };

    let res = select! {
        _ = tokio::time::sleep(timeout_duration) => {
            None
        }
        addr = find_wake_runner_address => {
            Some(addr)
        }
    };

    timeout.cancel();

    return res;
}

#[tokio::main()]
async fn main() -> Result<(), Box<dyn Error>> {
    // let config_str = fs::read_to_string("config.toml").await.unwrap();
    // println!("{:?}", config_str);

    // let config: Config = toml::from_str(&config_str).unwrap();

    // println!("{:?}", config);
    // Ok(())
    let my_mac_address = "b0:6e:bf:c5:5f:69";
    let target_address = my_mac_address.parse::<MacAddress>().unwrap();

    send_wake(target_address, "sleep").await;

    // let res = awake_wake_runner(target_address, Duration::from_secs(20)).await;
    // println!("Result of awaking: {res:?}");

    // println!("Waiting before exiting");
    // tokio::time::sleep(Duration::from_secs(20)).await;

    // let wake_runner_address = ping_for_wake_runner_address(target_address, Duration::from_secs(20))
    //     .await
    //     .ok_or("Could not get wake runner address")?;

    // let uri = match wake_runner_address {
    //     SocketAddr::V4(v4) => runner_uri_from_socket(v4),
    //     SocketAddr::V6(_) => todo!(),
    // };

    // println!("Wake runner is on: {uri}");
    // if let None = ping_wake_runner(uri).await {
    //     println!("Warning: ping failed");
    // }

    // send_wake_to_uri("sleep", &uri).await;

    // ping_for_wake_runner_address(target_address).await;
    // wake_on_lan::send(target_address).await?;
    Ok(())
}
