use std::{
    error::Error,
    io,
    net::{Ipv4Addr, SocketAddrV4},
};

use tokio::net::UdpSocket;

use super::interfaces::get_broadcastable_v4_interfaces;

const MAC_ADDRESS_SIZE: usize = 6;
const MAGIC_PACKET_TOTAL_SIZE_BYTES: usize = 102;
const HEADER_SIZE_BYTES: usize = 6;
const TARGET_MAC_ADDRESS_REPETITIONS: usize = 16;

pub async fn send(mac_address: [u8; MAC_ADDRESS_SIZE]) -> Result<(), Box<dyn Error>> {
    let interfaces = get_broadcastable_v4_interfaces()?;
    for interface in interfaces {
        send_from_to(mac_address, interface.ip, interface.broadcast.unwrap()).await?;
    }
    Ok(())
}

pub async fn send_from_to(
    mac_address: [u8; MAC_ADDRESS_SIZE],
    from: Ipv4Addr,
    to: Ipv4Addr,
) -> io::Result<()> {
    let sock = UdpSocket::bind(SocketAddrV4::new(from, 0)).await?;
    sock.set_broadcast(true)?;

    let payload = create_magic_packet(mac_address);

    let sent_bytes = sock.send_to(&payload, SocketAddrV4::new(to, 9)).await?;
    println!("Sent {:?} bytes from {:?}, to: {:?}", sent_bytes, from, to);

    Ok(())
}

fn create_magic_packet(mac_address: [u8; MAC_ADDRESS_SIZE]) -> [u8; MAGIC_PACKET_TOTAL_SIZE_BYTES] {
    let mut magic_packet_content = [0; MAGIC_PACKET_TOTAL_SIZE_BYTES];
    // Fill 6 bytes of 0xFF
    for i in 0..HEADER_SIZE_BYTES {
        magic_packet_content[i] = u8::MAX;
    }

    // Add 16 repetitions of the wakee:s mac address
    for i in 0..TARGET_MAC_ADDRESS_REPETITIONS {
        let offset = HEADER_SIZE_BYTES + i * mac_address.len();
        for (byte_idx, byte) in mac_address.iter().enumerate() {
            magic_packet_content[offset + byte_idx] = *byte;
        }
    }

    magic_packet_content
}
