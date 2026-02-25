use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use super::status::{HostInfo, HostStatus};

#[derive(Default, Serialize, Deserialize)]
pub struct AppState {
    pub hosts: Vec<HostInfo>,
    pub statuses: HashMap<String, HostStatus>,
}
