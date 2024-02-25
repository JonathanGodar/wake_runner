pub mod router;
pub mod start_wake_body_dto;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tokio::process::ChildStdout;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Wake {
    name: String,
    command: String,

    #[serde(default)]
    multiple_instances: bool,
    working_directory: Option<String>,

    #[serde(default)]
    arguments: Vec<String>,
}

#[derive(Debug)]
pub struct WakeProcess {
    pub id: String,
    pub name: String,
    pub std_out: ChildStdout,
}

pub type WakeProcessMap = HashMap<String, WakeProcess>;
