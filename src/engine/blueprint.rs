use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Blueprint {
    pub name: String,
    pub image: String,
    pub env: Vec<String>,
    pub mounts: Vec<Mount>,
    pub state: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Mount {
    pub source: String,
    pub destination: String,
}
