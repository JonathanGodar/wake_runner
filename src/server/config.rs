use std::{fmt, path::PathBuf};

use crate::server::wake;
use directories::ProjectDirs;
use futures::StreamExt;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub wakes: Vec<wake::Wake>,
}

impl Config {
    fn default() -> Config {
        Self { wakes: vec![] }
    }
}

static CONFIG_PATHS: OnceCell<Vec<PathBuf>> = OnceCell::new();

// const CONFIG_PATHS: &'static [&'static str] = &[
//     #[cfg(debug_assertions)]
//     {
//         "./runner_config.toml"
//     },
//     "~/.config/wake_runner/runner_config.toml",
// ];

pub async fn init_config() -> Config {
    let mut general_config_dir = ProjectDirs::from("com", "ngodag", "wake_runner")
        .unwrap()
        .config_dir()
        .to_owned();

    general_config_dir.push("config.toml");

    CONFIG_PATHS.set(vec![
        #[cfg(debug_assertions)]
        {
            PathBuf::from("./runner_config.toml")
        },
        general_config_dir,
    ]);

    println!("{:?}", CONFIG_PATHS);

    let maybe_config_str = Box::pin(
        futures::stream::iter(CONFIG_PATHS.get().unwrap())
            .filter_map(|path| async move { return fs::read_to_string(path).await.ok() }),
    )
    .next()
    .await;

    let config_str = match maybe_config_str {
        Some(config_str) => config_str,
        None => {
            let conf_str = toml::to_string(&Config::default()).unwrap();
            let write_result = write_config(&conf_str).await;
            if let Err(e) = write_result {
                println!("Error writing default config: {e:?}")
            }
            conf_str
        }
    };

    let config: Config = toml::from_str(&config_str).unwrap();
    config
}

async fn write_config(config_str: &str) -> Result<(), std::io::Error> {
    let config_path = &CONFIG_PATHS.get().unwrap()[0];
    println!("Config path {config_path:?}");

    tokio::fs::create_dir_all(config_path.parent().unwrap()).await?;
    tokio::fs::write(config_path, config_str).await
}
