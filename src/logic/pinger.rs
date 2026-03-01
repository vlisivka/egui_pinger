use crate::model::{AppState, HostInfo, PingMode};
use rand::RngExt;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use ping_async::{IcmpEchoRequestor, IcmpEchoStatus};

pub type SharedState = Arc<Mutex<AppState>>;

/// Returns a randomized ping interval for the given mode.
/// Each mode has ±5% jitter to defeat traffic analysis.
pub fn compute_interval(mode: PingMode, rng: &mut impl rand::Rng) -> Duration {
    let (base, jitter_range) = match mode {
        PingMode::VeryFast => (1.0, 0.05),
        PingMode::Fast => (2.0, 0.2),
        PingMode::NotFast => (5.0, 0.5),
        PingMode::Normal => (10.0, 1.0),
        PingMode::NotSlow => (30.0, 3.0),
        PingMode::Slow => (60.0, 5.0),
        PingMode::VerySlow => (300.0, 15.0),
    };
    let jitter: f64 = rng.random_range(-jitter_range..jitter_range);
    Duration::from_secs_f64(base + jitter)
}

/// Generates a randomized ICMP payload for the given host config.
pub fn generate_payload(host: &HostInfo) -> Vec<u8> {
    let mut rng = rand::rng();
    let mut size = host.packet_size.clamp(16, 1400);
    if host.random_padding {
        // Add 0-25% random extra padding
        let extra = rng.random_range(0..=(size / 4));
        size += extra;
    }
    (0..size).map(|_| rng.random()).collect()
}

/// Background task that pings all configured hosts at regular intervals.
pub async fn pinger_task(state: SharedState) {
    // Map of address -> next scheduled ping time
    let mut next_pings: HashMap<String, Instant> = HashMap::new();
    // Cache of ping-async requestors
    let mut requestors: HashMap<String, IcmpEchoRequestor> = HashMap::new();

    loop {
        // Check for hosts that are due for a ping
        let hosts_to_ping: Vec<HostInfo> = {
            let state_lock = state
                .lock()
                .expect("Failed to lock state for reading hosts");

            let now = Instant::now();
            let mut rng = rand::rng(); // Created and used only within this block
            state_lock
                .hosts
                .iter()
                .filter_map(|h| {
                    let next = next_pings.entry(h.address.clone()).or_insert(now);
                    if *next <= now {
                        let interval = compute_interval(h.mode, &mut rng);
                        *next = now + interval;
                        Some(h.clone())
                    } else {
                        None
                    }
                })
                .collect()
        };

        for host_info in hosts_to_ping {
            let address = host_info.address.clone();
            let state = state.clone();

            // Get or create requestor for this host
            let requestor = if let Some(r) = requestors.get(&address) {
                Some(r.clone())
            } else {
                // Resolve the address
                let clean_address = if address.starts_with('[') && address.ends_with(']') {
                    &address[1..address.len() - 1]
                } else {
                    &address
                };

                let ip = if let Ok(ip) = clean_address.parse::<IpAddr>() {
                    Some(ip)
                } else {
                    // Try DNS resolution
                    let lookup_str = format!("{}:0", address);
                    if let Ok(mut addrs) = tokio::net::lookup_host(&lookup_str).await {
                        addrs.next().map(|a| a.ip())
                    } else {
                        None
                    }
                };

                if let Some(target_ip) = ip {
                    match IcmpEchoRequestor::new(target_ip, None, None, None) {
                        Ok(r) => {
                            requestors.insert(address.clone(), r.clone());
                            Some(r)
                        }
                        Err(e) => {
                            eprintln!("Failed to create ICMP requestor for {}: {}", address, e);
                            None
                        }
                    }
                } else {
                    None
                }
            };

            if let Some(r) = requestor {
                tokio::spawn(async move {
                    let result = r.send().await;

                    let (alive, rtt_ms) = match result {
                        Ok(reply) => {
                            if reply.status() == IcmpEchoStatus::Success {
                                (true, reply.round_trip_time().as_secs_f64() * 1000.0)
                            } else {
                                (false, f64::NAN)
                            }
                        }
                        Err(_) => (false, f64::NAN),
                    };

                    let mut state_lock = state
                        .lock()
                        .expect("Failed to lock state for updating status");
                    if let Some(status) = state_lock.statuses.get_mut(&address) {
                        status.alive = alive;
                        status.add_sample(rtt_ms);
                    }
                });
            } else {
                let mut state_lock = state
                    .lock()
                    .expect("Failed to lock state for updating status");
                if let Some(status) = state_lock.statuses.get_mut(&address) {
                    status.alive = false;
                    status.add_sample(f64::NAN);
                }
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
#[path = "pinger_tests.rs"]
mod tests;
