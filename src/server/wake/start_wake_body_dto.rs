use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct StartWakeBody {
    pub name: String,
}
