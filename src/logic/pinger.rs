use crate::model::{AppState, HostInfo, PingMode};
use futures::future::join_all;
use rand::RngExt;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use surge_ping::ping;

pub type SharedState = Arc<Mutex<AppState>>;

/// Background task that pings all configured hosts at regular intervals.
pub async fn pinger_task(state: SharedState) {
    let payload = [42u8; 16];
    // Map of address -> next scheduled ping time
    let mut next_pings: HashMap<String, Instant> = HashMap::new();
    let mut rng = rand::rng();

    loop {
        // Check for hosts that are due for a ping
        let hosts_to_ping: Vec<HostInfo> = {
            let state_lock = state
                .lock()
                .expect("Failed to lock state for reading hosts");

            let now = Instant::now();
            state_lock
                .hosts
                .iter()
                .filter_map(|h| {
                    let next = next_pings.entry(h.address.clone()).or_insert(now);
                    if *next <= now {
                        // Schedule next ping
                        let interval = match h.mode {
                            PingMode::Fast => Duration::from_secs(1),
                            PingMode::Slow => {
                                // 60 seconds ± 5 seconds jitter
                                let jitter: f64 = rng.random_range(-5.0..5.0);
                                Duration::from_secs_f64(60.0 + jitter)
                            }
                        };
                        *next = now + interval;
                        Some(h.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        if !hosts_to_ping.is_empty() {
            let ping_tasks: Vec<_> = hosts_to_ping
                .into_iter()
                .map(|host_info| {
                    let address = host_info.address.clone();
                    let state = state.clone();

                    tokio::spawn(async move {
                        // Resolve the address (could be IP or domain)
                        let ip = match address.parse::<IpAddr>() {
                            Ok(ip) => Some(ip),
                            Err(_) => {
                                // Try DNS resolution
                                let lookup_str = format!("{}:0", address);
                                if let Ok(mut addrs) = tokio::net::lookup_host(&lookup_str).await {
                                    addrs.next().map(|a| a.ip())
                                } else {
                                    None
                                }
                            }
                        };

                        let (alive, rtt_ms) = if let Some(ip) = ip {
                            let result =
                                tokio::time::timeout(Duration::from_secs(2), ping(ip, &payload))
                                    .await;

                            match result {
                                Ok(Ok((_, duration))) => (true, duration.as_secs_f64() * 1000.0),
                                _ => (false, f64::NAN),
                            }
                        } else {
                            (false, f64::NAN) // Domain resolution failed
                        };

                        let mut state_lock = state
                            .lock()
                            .expect("Failed to lock state for updating status");
                        if let Some(status) = state_lock.statuses.get_mut(&address) {
                            status.alive = alive;
                            status.add_sample(rtt_ms);
                        }
                    })
                })
                .collect();

            // Run pings in parallel
            tokio::spawn(async move {
                let _res = join_all(ping_tasks).await;
            });
        }

        // Sleep for a short while before next check
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
