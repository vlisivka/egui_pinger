use super::status::{HostInfo, HostStatus, LogFilter};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Default, Serialize, Deserialize)]
pub struct AppState {
    pub hosts: Vec<HostInfo>,
    pub statuses: HashMap<String, HostStatus>,
    #[serde(default)]
    pub log_filter: LogFilter,
}
