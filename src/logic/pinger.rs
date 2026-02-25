use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use surge_ping::ping;
use futures::future::join_all;
use crate::model::{AppState, HostInfo};

pub type SharedState = Arc<Mutex<AppState>>;

/// Background task that pings all configured hosts at regular intervals.
pub async fn pinger_task(state: SharedState) {
    let payload = [42u8; 16];
    let mut interval = tokio::time::interval(Duration::from_secs(2));

    loop {
        interval.tick().await;

        let hosts: Vec<HostInfo> = {
            let state_lock = state.lock().expect("Failed to lock state for reading hosts");
            state_lock.hosts.clone()
        };
        
        if hosts.is_empty() {
            continue;
        }

        // Create and launch all pings in parallel tasks
        let ping_tasks: Vec<_> = hosts
            .iter()
            .filter_map(|host_info| {
                let ip: IpAddr = host_info.address.parse().ok()?;
                let payload = payload;
                let address = host_info.address.clone();
                let state = state.clone();

                Some(tokio::spawn(async move {
                    let result =
                        tokio::time::timeout(Duration::from_secs(2), ping(ip, &payload)).await;

                    let (alive, rtt_ms) = match result {
                        // Response received, host is alive
                        Ok(Ok((_, duration))) => (true, duration.as_secs_f64() * 1000.0),
                        // No response or error, host is down
                        _ => (false, f64::NAN),
                    };

                    let mut state_lock = state.lock().expect("Failed to lock state for updating status");
                    if let Some(status) = state_lock.statuses.get_mut(&address) {
                        status.alive = alive;
                        status.add_sample(rtt_ms);
                    }
                }))
            })
            .collect();

        // Run all tasks in parallel and wait for completion (non-blocking for the loop)
        tokio::spawn(async move {
            let _res = join_all(ping_tasks).await;
        });
    }
}
