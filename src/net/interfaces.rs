use std::error::Error;

use network_interface::{NetworkInterface, NetworkInterfaceConfig};

pub fn get_broadcastable_v4_interfaces() -> Result<Vec<network_interface::V4IfAddr>, Box<dyn Error>>
{
    let mut cast_to_networks = vec![];
    let ifs = NetworkInterface::show()?;
    for nif in ifs {
        for addr in &nif.addr {
            match addr {
                network_interface::Addr::V4(v4_addr) => {
                    if v4_addr.ip.is_loopback() || v4_addr.broadcast.is_none() {
                        continue;
                    }

                    cast_to_networks.push(v4_addr.clone());
                }
                _ => {}
            }
        }
        println!("{:?}", nif.addr);
    }

    Ok(cast_to_networks)
}
