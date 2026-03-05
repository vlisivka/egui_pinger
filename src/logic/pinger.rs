use crate::constants::{
    HOP_DATA_FRESHNESS_SEC, STATE_CONFIRMATION_STREAK, STATS_SNAPSHOT_INTERVAL,
    TRACEROUTE_INTERVAL_SEC, TRACEROUTE_MIN_COOLDOWN_SEC,
};
use crate::logic::tracer::run_traceroute;
use crate::model::{AppState, HostInfo, LogEntry, PingMode};
use ping_async::{IcmpEchoRequestor, IcmpEchoStatus};
use rand::RngExt;
use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
/// Generates a randomized ICMP payload.
pub fn generate_payload(host: Option<&HostInfo>) -> Vec<u8> {
    let mut rng = rand::rng();
    let (mut size, random_padding) = match host {
        Some(h) => (h.packet_size.clamp(16, 1400), h.random_padding),
        None => (16, false),
    };
    if random_padding {
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
    let requestors: Arc<tokio::sync::Mutex<HashMap<String, IcmpEchoRequestor>>> =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));

    // Map to keep track of when we last ran traceroute per target
    let mut last_trace_times: HashMap<String, Instant> = HashMap::new();

    loop {
        let now = Instant::now();

        // 1. Check for targets that need traceroute (newly added or expired > 1h)
        let targets_needing_trace: Vec<String> = {
            let mut state_lock = state.lock().expect("Failed to lock state");
            let target_addrs: Vec<String> =
                state_lock.hosts.iter().map(|h| h.address.clone()).collect();
            let mut needing_trace = Vec::new();

            for address in target_addrs {
                if let Some(host_info) = state_lock.hosts.iter().find(|h| h.address == address)
                    && host_info.is_stopped
                {
                    continue;
                }
                if let Some(status) = state_lock.statuses.get_mut(&address) {
                    if status.tracer_in_progress {
                        continue;
                    }
                    let just_became_down =
                        !status.streak_success && status.streak == STATE_CONFIRMATION_STREAK;
                    let just_became_up =
                        status.streak_success && status.streak == STATE_CONFIRMATION_STREAK;
                    let overdue = match last_trace_times.get(&address) {
                        Some(last) => {
                            let duration = now.duration_since(*last);
                            duration > Duration::from_secs(TRACEROUTE_INTERVAL_SEC)
                                || ((just_became_down || just_became_up)
                                    && duration > Duration::from_secs(TRACEROUTE_MIN_COOLDOWN_SEC))
                        }
                        None => true,
                    };
                    let forced = status.manual_trace_requested;
                    if overdue || forced {
                        status.tracer_in_progress = true;
                        status.manual_trace_requested = false;
                        needing_trace.push(address);
                    }
                }
            }
            needing_trace
        };

        for target_addr in targets_needing_trace {
            let state_c = state.clone();
            let addr_c = target_addr.clone();
            last_trace_times.insert(target_addr.clone(), now);

            tokio::spawn(async move {
                let hops = run_traceroute(&addr_c).await;
                let mut state_lock = state_c.lock().expect("Failed to lock state after trace");

                if let Some(status) = state_lock.statuses.get_mut(&addr_c) {
                    // Refined update logic:
                    // 1. Never overwrite with an empty path.
                    // 2. If the new path is more complete (last hop is the target), always take it.
                    // 3. Otherwise, only take it if it's longer or we don't have a path yet.
                    // 4. This prevents flickering and losing path info when internet is flaky.
                    let new_reaches_target = hops.last().map(|h| h == &addr_c).unwrap_or(false);
                    let old_reaches_target = status
                        .traceroute_path
                        .last()
                        .map(|h| h == &addr_c)
                        .unwrap_or(false);

                    let should_update = !hops.is_empty()
                        && (status.traceroute_path.is_empty()
                            || (new_reaches_target && !old_reaches_target)
                            || (new_reaches_target && hops.len() >= status.traceroute_path.len())
                            || (!old_reaches_target && hops.len() >= status.traceroute_path.len())
                            || (status.alive && new_reaches_target));

                    if should_update {
                        let now_ts = chrono::Utc::now().timestamp() as u64;
                        status.events.push_back(LogEntry::RouteUpdate {
                            timestamp: now_ts,
                            path: hops.clone(),
                        });
                        status.trim_events();
                        status.traceroute_path = hops.clone();
                    }
                    status.last_traceroute = Some(Instant::now());
                    status.tracer_in_progress = false;
                }

                // Add all discovered hops to the global ping pool (statuses)
                for hop_addr in hops {
                    let normalized_hop = hop_addr.trim().to_lowercase();
                    state_lock
                        .statuses
                        .entry(normalized_hop)
                        .or_insert_with(|| crate::model::HostStatus {
                            is_trace_hop: true,
                            ..Default::default()
                        })
                        .dependent_targets
                        .insert(addr_c.clone());
                }
            });
        }

        // 2. Identify DOWN targets and activate diagnostic on hops
        {
            let mut state_lock = state
                .lock()
                .expect("Failed to lock state for diagnostic check");
            let target_alive: HashMap<String, bool> = state_lock
                .hosts
                .iter()
                .map(|h| {
                    (
                        h.address.clone(),
                        if h.is_stopped {
                            true
                        } else {
                            state_lock
                                .statuses
                                .get(&h.address)
                                .map(|s| s.alive)
                                .unwrap_or(true)
                        },
                    )
                })
                .collect();

            // Update diagnostic_mode for all hops
            for (_addr, status) in state_lock.statuses.iter_mut() {
                if status.is_trace_hop {
                    // Activate diagnostic if ANY dependent target is DOWN
                    status.diagnostic_mode = status
                        .dependent_targets
                        .iter()
                        .any(|t| !target_alive.get(t).unwrap_or(&true));
                }
            }
        }

        // 3. Check for addresses that are due for a ping
        let addresses_to_ping: Vec<(String, PingMode, Option<HostInfo>)> = {
            let state_lock = state
                .lock()
                .expect("Failed to lock state for reading hosts");

            let now = Instant::now();
            let mut rng = rand::rng();

            // Build a lookup for target hosts to know their desired mode and config
            let target_configs: HashMap<String, HostInfo> = state_lock
                .hosts
                .iter()
                .map(|h| (h.address.clone(), h.clone()))
                .collect();

            state_lock
                .statuses
                .iter()
                .filter_map(|(addr, status)| {
                    let next = next_pings.entry(addr.clone()).or_insert(now);
                    let host_info = target_configs.get(addr);

                    if let Some(h) = host_info
                        && h.is_stopped
                    {
                        return None;
                    }

                    // Effective mode logic:
                    // - If diagnostic_mode is ON -> Fast (2s)
                    // - Else if it's a user target -> host.mode
                    // - Else (pure hop) -> Slow (60s)
                    let mode = if status.diagnostic_mode {
                        PingMode::Fast
                    } else if let Some(h) = host_info {
                        h.mode
                    } else {
                        PingMode::Slow
                    };

                    let interval = compute_interval(mode, &mut rng);

                    // If the new mode's interval is shorter than the remaining wait time,
                    // jump the queue and ping now/soon, rather than waiting the full Slow interval.
                    // This handles switching from Slow to Fast correctly without race conditions.
                    let max_wait = Duration::from_secs(
                        match mode {
                            PingMode::VeryFast => 1,
                            PingMode::Fast => 2,
                            PingMode::NotFast => 5,
                            PingMode::Normal => 10,
                            PingMode::NotSlow => 30,
                            PingMode::Slow => 60,
                            PingMode::VerySlow => 300,
                        } + 2,
                    );

                    if now + max_wait < *next {
                        *next = now;
                    }

                    if *next <= now {
                        *next = now + interval;
                        Some((addr.clone(), mode, host_info.cloned()))
                    } else {
                        None
                    }
                })
                .collect()
        };

        // 4. Failure deduction logic (Phase 3 Safeguards)
        {
            let mut state_lock = state
                .lock()
                .expect("Failed to lock state for failure detection");

            // Extract all target addresses
            let target_addresses: Vec<String> =
                state_lock.hosts.iter().map(|h| h.address.clone()).collect();

            for target_addr in target_addresses {
                let is_stopped = state_lock
                    .hosts
                    .iter()
                    .find(|h| h.address == target_addr)
                    .map(|h| h.is_stopped)
                    .unwrap_or(false);
                let target_down = if is_stopped {
                    false
                } else {
                    state_lock
                        .statuses
                        .get(&target_addr)
                        .map(|s| !s.streak_success && s.streak >= STATE_CONFIRMATION_STREAK)
                        .unwrap_or(false)
                };

                if target_down {
                    let path = state_lock
                        .statuses
                        .get(&target_addr)
                        .map(|s| s.traceroute_path.clone())
                        .unwrap_or_default();
                    let mut found_point = None;

                    // If route is totally empty, or only contains the target itself, and we are DOWN, it's a local breakdown
                    if path.is_empty() || (path.len() == 1 && path[0] == target_addr) {
                        found_point = Some(String::from("Local Interface"));
                    } else {
                        // Check if the first hop (gateway) is broken, which also means local breakdown
                        let mut gateway_broken = false;
                        if let Some(first_hop_status) = state_lock.statuses.get(&path[0]) {
                            let data_is_fresh = first_hop_status
                                .last_updated
                                .map(|t| {
                                    now.duration_since(t)
                                        < Duration::from_secs(HOP_DATA_FRESHNESS_SEC)
                                })
                                .unwrap_or(false);
                            let hop_is_broken = !first_hop_status.streak_success
                                && first_hop_status.streak >= STATE_CONFIRMATION_STREAK;
                            let hop_is_stale = !data_is_fresh;

                            if hop_is_broken || hop_is_stale {
                                gateway_broken = true;
                            }
                        }

                        if gateway_broken {
                            found_point = Some(String::from("Local Interface"));
                        } else {
                            // "At least one rule": if streak of failures is < 3 and data is fresh, we consider it "congested" but ALIVE
                            for (hop_idx, hop) in path.iter().enumerate() {
                                if let Some(h_status) = state_lock.statuses.get(hop) {
                                    let data_is_fresh = h_status
                                        .last_updated
                                        .map(|t| {
                                            now.duration_since(t)
                                                < Duration::from_secs(HOP_DATA_FRESHNESS_SEC)
                                        })
                                        .unwrap_or(false);
                                    let hop_is_broken = !h_status.streak_success
                                        && h_status.streak >= STATE_CONFIRMATION_STREAK;
                                    let hop_is_stale = !data_is_fresh;

                                    if hop_is_broken || hop_is_stale {
                                        // Double check: are all subsequent hops also broken?
                                        let mut all_further_broken = true;
                                        for next_hop in &path[hop_idx + 1..] {
                                            if let Some(nh_status) =
                                                state_lock.statuses.get(next_hop)
                                            {
                                                let nh_data_is_fresh = nh_status
                                                    .last_updated
                                                    .map(|t| {
                                                        now.duration_since(t)
                                                            < Duration::from_secs(
                                                                HOP_DATA_FRESHNESS_SEC,
                                                            )
                                                    })
                                                    .unwrap_or(false);
                                                let nh_is_stale = !nh_data_is_fresh;

                                                if (nh_status.streak_success
                                                    || nh_status.streak < STATE_CONFIRMATION_STREAK)
                                                    && !nh_is_stale
                                                {
                                                    all_further_broken = false;
                                                    break;
                                                }
                                            } else {
                                                // Unknown hop is assumed unbroken to avoid false positives midway
                                                all_further_broken = false;
                                                break;
                                            }
                                        }

                                        if all_further_broken {
                                            found_point = Some(hop.clone());
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if let Some(status) = state_lock.statuses.get_mut(&target_addr) {
                        status.failure_point = found_point;
                    }
                } else if let Some(status) = state_lock.statuses.get_mut(&target_addr) {
                    status.failure_point = None;
                }
            }
        }

        for (address, _mode, host_info) in addresses_to_ping {
            let state = state.clone();
            let requestors_clone = requestors.clone();
            let _payload = generate_payload(host_info.as_ref());

            tokio::spawn(async move {
                // Get or create requestor for this host
                let requestor_opt = {
                    let mut reqs = requestors_clone.lock().await;
                    if let Some(r) = reqs.get(&address) {
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
                                    reqs.insert(address.clone(), r.clone());
                                    Some(r)
                                }
                                Err(e) => {
                                    eprintln!(
                                        "Failed to create ICMP requestor for {}: {}",
                                        address, e
                                    );
                                    None
                                }
                            }
                        } else {
                            None
                        }
                    }
                };

                if let Some(r) = requestor_opt {
                    let result = r.send().await;

                    let (alive, rtt_ms) = match result {
                        Ok(reply) => {
                            if reply.status() == IcmpEchoStatus::Success {
                                (true, reply.round_trip_time().as_secs_f64() * 1000.0)
                            } else {
                                (false, f64::NAN)
                            }
                        }
                        Err(_) => {
                            // Invalidate cache on err (e.g., interface change)
                            let mut reqs = requestors_clone.lock().await;
                            reqs.remove(&address);
                            (false, f64::NAN)
                        }
                    };

                    let mut state_lock = state
                        .lock()
                        .expect("Failed to lock state for updating status");
                    if let Some(status) = state_lock.statuses.get_mut(&address) {
                        status.add_sample(rtt_ms, alive);

                        let now_ts = chrono::Utc::now().timestamp() as u64;
                        let mut extra_events: Vec<LogEntry> = Vec::new();

                        // 1. Log the ping result
                        let entry = LogEntry::Ping {
                            timestamp: now_ts,
                            seq: status.sent,
                            rtt: if alive { Some(rtt_ms as f32) } else { None },
                            bytes: host_info
                                .as_ref()
                                .map(|h| h.packet_size as u16)
                                .unwrap_or(16),
                        };
                        status.events.push_back(entry.clone());

                        // 2. Incident Detection (Loss/Restoration)
                        if !alive
                            && status.streak == STATE_CONFIRMATION_STREAK
                            && status.incident_start.is_none()
                        {
                            // Just became "down" officially after 3 failures
                            status.incident_start = Some(now_ts);
                            let ev = LogEntry::Incident {
                                timestamp: now_ts,
                                is_break: true,
                                streak: status.streak,
                                downtime_sec: None,
                                node: status.failure_point.clone(),
                            };
                            status.events.push_back(ev.clone());
                            extra_events.push(ev);
                        } else if alive && status.incident_start.is_some() {
                            // Just restored from being officially "down"
                            let downtime = status.incident_start.map(|s| now_ts.saturating_sub(s));
                            let ev = LogEntry::Incident {
                                timestamp: now_ts,
                                is_break: false,
                                streak: status.streak,
                                downtime_sec: downtime,
                                node: None,
                            };
                            status.events.push_back(ev.clone());
                            extra_events.push(ev);
                            status.incident_start = None;
                        }
                        status.prev_alive = Some(alive);

                        // 3. Statistics every 300 pings
                        status.log_pings_since_stats += 1;
                        if status.log_pings_since_stats >= STATS_SNAPSHOT_INTERVAL {
                            let entry = LogEntry::Statistics {
                                timestamp: now_ts,
                                mean: status.mean as f32,
                                median: status.median as f32,
                                p95: status.p95 as f32,
                                jitter: status.rtp_jitter as f32,
                                mos: status.mos as f32,
                                loss: (status.lost as f32 / status.sent as f32) * 100.0,
                                sent: status.sent,
                                lost: status.lost,
                                rtp_mean_jitter: status.rtp_jitter_mean as f32,
                                rtp_median_jitter: status.rtp_jitter_median as f32,
                                availability: status.availability as f32,
                                outliers: status.outliers,
                                streak: status.streak,
                                stddev: status.stddev as f32,
                                min_rtt: status.min_rtt as f32,
                                max_rtt: status.max_rtt as f32,
                            };
                            status.events.push_back(entry.clone());
                            extra_events.push(entry);
                            status.log_pings_since_stats = 0;
                        }

                        // Cap buffer size
                        status.trim_events();

                        // 4. File Logging
                        if let Some(ref h) = host_info {
                            let mut lines = vec![entry.format(&address, Some(&h.display))];
                            for ev in extra_events {
                                lines.push(ev.format(&address, Some(&h.display)));
                            }
                            h.append_to_log(&lines);
                        }
                    }
                } else {
                    let mut state_lock = state
                        .lock()
                        .expect("Failed to lock state for updating status");
                    if let Some(status) = state_lock.statuses.get_mut(&address) {
                        status.add_sample(f64::NAN, false);

                        let now_ts = chrono::Utc::now().timestamp() as u64;
                        let (ping_entry, incident_entry) = {
                            let pe = LogEntry::Ping {
                                timestamp: now_ts,
                                seq: status.sent,
                                rtt: None,
                                bytes: host_info
                                    .as_ref()
                                    .map(|h| h.packet_size as u16)
                                    .unwrap_or(16),
                            };
                            status.events.push_back(pe.clone());

                            // Incident detection for unresolved/unpingable
                            let mut ie = None;
                            if status.streak == STATE_CONFIRMATION_STREAK
                                && status.incident_start.is_none()
                            {
                                status.incident_start = Some(now_ts);
                                let e = LogEntry::Incident {
                                    timestamp: now_ts,
                                    is_break: true,
                                    streak: status.streak,
                                    downtime_sec: None,
                                    node: status.failure_point.clone(),
                                };
                                status.events.push_back(e.clone());
                                ie = Some(e);
                            }
                            status.prev_alive = Some(false);
                            (pe, ie)
                        };

                        status.trim_events();

                        if let Some(ref h) = host_info {
                            let mut lines = vec![ping_entry.format(&address, Some(&h.display))];
                            if let Some(ie) = incident_entry {
                                lines.push(ie.format(&address, Some(&h.display)));
                            }
                            h.append_to_log(&lines);
                        }
                    }
                }
            });
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
#[path = "pinger_tests.rs"]
mod tests;
