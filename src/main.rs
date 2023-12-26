use mac_address::MacAddress;
use serde::{Deserialize, Serialize};
use std::error::Error;
use tokio::fs;

mod wake_on_lan;

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    mac_address: String,
}

#[tokio::main()]
async fn main() -> Result<(), Box<dyn Error>> {
    // let config_str = fs::read_to_string("config.toml").await.unwrap();
    // println!("{:?}", config_str);

    // let config: Config = toml::from_str(&config_str).unwrap();

    // println!("{:?}", config);
    // Ok(())
    let my_mac_address = "b0:6e:bf:c5:5f:69";
    let target_address = my_mac_address.parse::<MacAddress>().unwrap().bytes();

    wake_on_lan::send(target_address).await?;
    Ok(())
}
