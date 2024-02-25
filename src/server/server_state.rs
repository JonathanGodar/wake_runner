use tokio::sync::watch;

use super::{config::Config, wake::WakeProcessMap};
use std::{collections::HashMap, sync::Mutex};

pub struct ServerState {
    pub config: Config,
    pub wake_processes: Mutex<WakeProcessMap>,
    pub active_wake_process_count_setter: Mutex<watch::Sender<usize>>,
    pub active_wake_process_count: watch::Receiver<usize>,
}
