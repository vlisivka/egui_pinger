use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use tr::tr;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PingMode {
    VeryFast, // 1s
    Fast,     // 2s
    NotFast,  // 5s
    Normal,   // 10s
    NotSlow,  // 30s
    Slow,     // 1m
    VerySlow, // 5m
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogEntry {
    /// Successful ping or timeout
    Ping {
        timestamp: u64,
        seq: u32,
        rtt: Option<f32>, // None = Timeout
        bytes: u16,
    },
    /// Periodic statistics
    Statistics {
        timestamp: u64,
        mean: f32,
        median: f32,
        p95: f32,
        jitter: f32,
        mos: f32,
        loss: f32,
        sent: u32,
        lost: u32,
        rtp_mean_jitter: f32,
        rtp_median_jitter: f32,
        availability: f32,
        outliers: u32,
        streak: u32,
        stddev: f32,
        min_rtt: f32,
        max_rtt: f32,
    },
    /// Route update
    RouteUpdate { timestamp: u64, path: Vec<String> },
    /// Incident (Loss/Restoration)
    Incident {
        timestamp: u64,
        is_break: bool, // true = loss, false = restoration
        streak: u32,    // number of packets in current state
        downtime_sec: Option<u64>,
        node: Option<String>,
    },
    /// Custom message marker
    Marker { timestamp: u64, message: String },
}

impl LogEntry {
    pub fn format(&self, address: &str, display: Option<&DisplaySettings>) -> String {
        let ts = if let Some(dt) = chrono::DateTime::from_timestamp(self.timestamp() as i64, 0) {
            let local_dt: chrono::DateTime<chrono::Local> = dt.into();
            local_dt.format("%Y-%m-%d %H:%M:%S").to_string()
        } else {
            "????-??-?? ??:??:??".to_string()
        };

        match self {
            LogEntry::Ping {
                seq, rtt, bytes, ..
            } => {
                if let Some(rtt_val) = rtt {
                    format!(
                        "[{}] {} {} {}: icmp_seq={} {}={:.1} {}",
                        ts,
                        bytes,
                        tr!("bytes from"),
                        address,
                        seq,
                        tr!("time"),
                        rtt_val,
                        tr!("ms")
                    )
                } else {
                    format!("[{}] {} icmp_seq={}", ts, tr!("Request timeout for"), seq)
                }
            }
            LogEntry::Statistics {
                mean,
                median,
                p95,
                jitter,
                mos,
                loss,
                sent,
                lost,
                rtp_mean_jitter,
                rtp_median_jitter,
                availability,
                outliers,
                streak,
                stddev,
                min_rtt,
                max_rtt,
                ..
            } => {
                let mut parts = Vec::new();

                let d_def = DisplaySettings::default();
                let d = display.unwrap_or(&d_def);

                if d.show_mean {
                    parts.push(format!("{}={:.1}{}", tr!("M"), mean, tr!("ms")));
                }
                if d.show_median {
                    parts.push(format!("{}={:.1}{}", tr!("Med"), median, tr!("ms")));
                }
                if d.show_p95 {
                    parts.push(format!("{}={:.1}{}", tr!("95%"), p95, tr!("ms")));
                }
                if d.show_rtp_jitter {
                    parts.push(format!("{}={:.1}{}", tr!("J"), jitter, tr!("ms")));
                }
                if d.show_rtp_mean_jitter {
                    parts.push(format!("{}={:.1}{}", tr!("Jm"), rtp_mean_jitter, tr!("ms")));
                }
                if d.show_rtp_median_jitter {
                    parts.push(format!(
                        "{}={:.1}{}",
                        tr!("Jmed"),
                        rtp_median_jitter,
                        tr!("ms")
                    ));
                }
                if d.show_mos {
                    parts.push(format!("{}={:.1}", tr!("MOS"), mos));
                }
                if d.show_availability {
                    parts.push(format!("{}={:.0}%", tr!("Av"), availability));
                }
                if d.show_outliers {
                    parts.push(format!("{}={}", tr!("Out"), outliers));
                }
                if d.show_streak {
                    parts.push(format!("{}={}", tr!("Str"), streak));
                }
                if d.show_stddev {
                    parts.push(format!("{}={:.1}", tr!("SD"), stddev));
                }
                if d.show_min_max {
                    parts.push(format!("{}={:.0}-{:.0}", tr!("m/M"), min_rtt, max_rtt));
                }
                if d.show_loss {
                    parts.push(format!("{}:{:.1}% ({}/{})", tr!("L"), loss, lost, sent));
                }

                if parts.is_empty() {
                    // Fallback to minimal info if everything is disabled
                    parts.push(format!("{}:{:.1}% ({}/{})", tr!("L"), loss, lost, sent));
                }

                format!("# [{}] {}: {}", ts, tr!("Statistics"), parts.join(" "))
            }
            LogEntry::RouteUpdate { path, .. } => {
                format!("% [{}] {}: {}", ts, tr!("Route updated"), path.join(" → "))
            }
            LogEntry::Incident {
                is_break,
                streak,
                downtime_sec,
                node,
                ..
            } => {
                if *is_break {
                    if let Some(n) = node {
                        let translated_node = if n == "Local Interface" {
                            tr!("Local Interface").to_string()
                        } else {
                            n.clone()
                        };
                        format!(
                            "! [{}] {} ({})",
                            ts,
                            tr!("Connectivity lost at {node}").replace("{node}", &translated_node),
                            tr!("{n} requests without answer").replace("{n}", &streak.to_string())
                        )
                    } else {
                        format!(
                            "! [{}] {} ({})",
                            ts,
                            tr!("Connectivity lost"),
                            tr!("{n} requests without answer").replace("{n}", &streak.to_string())
                        )
                    }
                } else {
                    let downtime_str = if let Some(d) = downtime_sec {
                        format!(" ({}={}{} )", tr!("downtime"), d, tr!("s"))
                    } else {
                        "".to_string()
                    };
                    format!(
                        "! [{}] {}{}",
                        ts,
                        tr!("Connectivity restored"),
                        downtime_str
                    )
                }
            }
            LogEntry::Marker { message, timestamp } => {
                let dts = if let Some(dt) = chrono::DateTime::from_timestamp(*timestamp as i64, 0) {
                    let local_dt: chrono::DateTime<chrono::Local> = dt.into();
                    local_dt.format("%Y-%m-%d %H:%M:%S").to_string()
                } else {
                    ts
                };
                format!("=== {}: {} ===", message, dts)
            }
        }
    }

    pub fn timestamp(&self) -> u64 {
        match self {
            LogEntry::Ping { timestamp, .. } => *timestamp,
            LogEntry::Statistics { timestamp, .. } => *timestamp,
            LogEntry::RouteUpdate { timestamp, .. } => *timestamp,
            LogEntry::Incident { timestamp, .. } => *timestamp,
            LogEntry::Marker { timestamp, .. } => *timestamp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplaySettings {
    #[serde(default = "default_true")]
    pub show_name: bool,
    #[serde(default = "default_true")]
    pub show_address: bool,
    #[serde(default = "default_true")]
    pub show_latency: bool,
    #[serde(default = "default_true")]
    pub show_mean: bool,
    #[serde(default = "default_true")]
    pub show_median: bool,
    #[serde(default = "default_true")]
    pub show_rtp_jitter: bool,
    #[serde(default = "default_false")]
    pub show_rtp_mean_jitter: bool,
    #[serde(default = "default_false")]
    pub show_rtp_median_jitter: bool,
    #[serde(default = "default_false")]
    pub show_mos: bool,
    #[serde(default = "default_false")]
    pub show_availability: bool,
    #[serde(default = "default_false")]
    pub show_outliers: bool,
    #[serde(default = "default_false")]
    pub show_streak: bool,
    #[serde(default = "default_false")]
    pub show_stddev: bool,
    #[serde(default = "default_false")]
    pub show_p95: bool,
    #[serde(default = "default_false")]
    pub show_min_max: bool,
    #[serde(default = "default_true")]
    pub show_loss: bool,
}

fn default_true() -> bool {
    true
}
fn default_false() -> bool {
    false
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            show_name: true,
            show_address: true,
            show_latency: true,
            show_mean: true,
            show_median: true,
            show_rtp_jitter: true,
            show_rtp_mean_jitter: false,
            show_rtp_median_jitter: false,
            show_mos: true,
            show_availability: false,
            show_outliers: false,
            show_streak: false,
            show_stddev: false,
            show_p95: false,
            show_min_max: false,
            show_loss: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogFilter {
    #[serde(default = "default_true")]
    pub show_pings: bool,
    #[serde(default = "default_true")]
    pub show_timeouts: bool,
    #[serde(default = "default_true")]
    pub show_stats: bool,
    #[serde(default = "default_true")]
    pub show_route: bool,
    #[serde(default = "default_true")]
    pub show_incidents: bool,
}

impl Default for LogFilter {
    fn default() -> Self {
        Self {
            show_pings: true,
            show_timeouts: true,
            show_stats: true,
            show_route: true,
            show_incidents: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostInfo {
    pub name: String,
    pub address: String,
    #[serde(default = "default_ping_mode")]
    pub mode: PingMode,
    #[serde(default)]
    pub display: DisplaySettings,
    #[serde(default = "default_packet_size")]
    pub packet_size: usize,
    #[serde(default = "default_false")]
    pub random_padding: bool,
    #[serde(default = "default_false")]
    pub log_to_file: bool,
    #[serde(default)]
    pub log_file_path: String,
    #[serde(default = "default_false")]
    pub is_stopped: bool,
}

fn default_ping_mode() -> PingMode {
    PingMode::Fast
}

fn default_packet_size() -> usize {
    16
}

impl HostInfo {
    pub fn is_local(&self) -> bool {
        if let Ok(ip) = self.address.parse::<std::net::IpAddr>() {
            match ip {
                std::net::IpAddr::V4(v4) => {
                    // RFC 1918: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
                    // as well as loopback and link-local
                    v4.is_private() || v4.is_loopback() || v4.is_link_local()
                }
                std::net::IpAddr::V6(v6) => {
                    // IPv6 Unique Local Address (fc00::/7) and loopback/link-local
                    v6.is_loopback()
                        || (v6.segments()[0] & 0xfe00 == 0xfc00)
                        || v6.is_unicast_link_local()
                }
            }
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostStatus {
    /// Whether we received a response from the host this time
    #[serde(skip, default)]
    pub alive: bool,
    /// Last RTT in milliseconds
    #[serde(skip, default)]
    pub latency: f64,
    /// Last 99 RTTs in milliseconds (NaN = loss)
    #[serde(skip, default)]
    pub history: Vec<f64>,
    /// Mean of latency
    #[serde(skip, default)]
    pub mean: f64,
    /// Standard RTP Jitter according to RFC 3550
    #[serde(skip, default)]
    pub rtp_jitter: f64,
    /// History of RTP Jitter values (last 99)
    #[serde(skip, default)]
    pub rtp_jitter_history: Vec<f64>,
    /// Median RTT
    #[serde(skip, default)]
    pub median: f64,
    /// 95th percentile of RTT
    #[serde(skip, default)]
    pub p95: f64,
    /// Standard deviation of RTT
    #[serde(skip, default)]
    pub stddev: f64,
    /// Minimum RTT in current history
    #[serde(skip, default)]
    pub min_rtt: f64,
    /// Maximum RTT in current history
    #[serde(skip, default)]
    pub max_rtt: f64,
    /// Mean of RTP Jitter history
    #[serde(skip, default)]
    pub rtp_jitter_mean: f64,
    /// Median of RTP Jitter history
    #[serde(skip, default)]
    pub rtp_jitter_median: f64,
    /// MOS (Mean Opinion Score) 1.0 - 4.5
    #[serde(skip, default)]
    pub mos: f64,
    /// Availability percentage based on all sent packets
    #[serde(skip, default)]
    pub availability: f64,
    /// Number of packets with RTT > mean + 3*stddev
    #[serde(skip, default)]
    pub outliers: u32,
    /// Current success/fail streak count
    #[serde(skip, default)]
    pub streak: u32,
    /// Whether the current streak is success (true) or fail (false)
    #[serde(skip, default)]
    pub streak_success: bool,
    /// Number of packets sent
    #[serde(skip, default)]
    pub sent: u32,
    /// Number of responses not received
    #[serde(skip, default)]
    pub lost: u32,

    // --- Traceroute & Unified Pool Fields ---
    /// The discovered sequence of IP addresses to reach this host
    #[serde(skip, default)]
    pub traceroute_path: Vec<String>,

    /// Whether this status entry is a transit hop (not an explicit target)
    #[serde(skip, default)]
    pub is_trace_hop: bool,

    /// List of target addresses that pass through this node
    #[serde(skip, default)]
    pub dependent_targets: HashSet<String>,

    /// List of targets for which this node is currently a confirmed failure point
    #[serde(skip, default)]
    pub failure_point_for: Vec<String>,

    /// Flag indicating that we are performing high-frequency diagnostic pings on this hop
    #[serde(skip, default)]
    pub diagnostic_mode: bool,

    /// The specific hop where the connection is currently broken (if any)
    #[serde(skip, default)]
    pub failure_point: Option<String>,

    /// Weather we are currently running a traceroute for this host
    #[serde(skip, default)]
    pub tracer_in_progress: bool,

    /// Manual request to rerun traceroute
    #[serde(skip, default)]
    pub manual_trace_requested: bool,

    /// Timestamp of the last ping result
    #[serde(skip, default)]
    pub last_updated: Option<std::time::Instant>,

    /// Timestamp of the last successful traceroute discovery
    #[serde(skip, default)]
    pub last_traceroute: Option<std::time::Instant>,

    /// Typed event log (up to 100,000 events)
    #[serde(skip, default)]
    pub events: VecDeque<LogEntry>,

    /// Previous alive state for incident detection
    #[serde(skip, default)]
    pub prev_alive: Option<bool>,

    /// Timestamp of the start of the current incident
    #[serde(skip, default)]
    pub incident_start: Option<u64>,

    /// Counter of pings since last statistics entry in log
    #[serde(skip, default)]
    pub log_pings_since_stats: u32,
}

impl HostStatus {
    /// Adds a new RTT sample and updates statistics.
    pub fn add_sample(&mut self, rtt_ms: f64, alive: bool) {
        self.sent += 1;
        self.alive = alive;
        self.last_updated = Some(std::time::Instant::now());

        if rtt_ms.is_nan() {
            self.lost += 1;
            if !self.streak_success {
                self.streak += 1;
            } else {
                self.streak = 1;
                self.streak_success = false;
            }
        } else if self.streak_success {
            self.streak += 1;
        } else {
            self.streak = 1;
            self.streak_success = true;
        }

        self.latency = rtt_ms;

        // Add to history (maximum 300 samples)
        self.history.push(rtt_ms);
        if self.history.len() > 300 {
            self.history.remove(0);
        }

        // Availability is calculated as a sliding window (unlike total Packet Loss)
        let total_window = self.history.len();
        if total_window > 0 {
            let lost_in_window = self.history.iter().filter(|rtt| rtt.is_nan()).count();
            self.availability =
                (total_window - lost_in_window) as f64 / total_window as f64 * 100.0;
        } else {
            self.availability = 100.0;
        }

        let valid_data: Vec<f64> = self
            .history
            .iter()
            .copied()
            .filter(|v| !v.is_nan())
            .collect();

        if valid_data.is_empty() {
            self.mean = 0.0;
            self.median = 0.0;
            return;
        }

        if valid_data.len() < 2 {
            self.mean = valid_data[0];
            self.median = valid_data[0];
            self.min_rtt = valid_data[0];
            self.max_rtt = valid_data[0];
            self.stddev = 0.0;
            self.p95 = valid_data[0];
            self.outliers = 0;
            self.mos = calculate_mos(self.mean, self.rtp_jitter, 100.0 - self.availability);
            return;
        }

        // Arithmetic mean
        self.mean = valid_data.iter().sum::<f64>() / valid_data.len() as f64;

        // Calculate RTP Jitter (RFC 3550)
        // J = J + (|D| - J) / 16
        // We calculate D as the difference in RTT between current and previous packet.
        if valid_data.len() >= 2 {
            let last_idx = valid_data.len() - 1;
            let current_rtt = valid_data[last_idx];
            let prev_rtt = valid_data[last_idx - 1];
            let d = (current_rtt - prev_rtt).abs();

            if self.rtp_jitter_history.is_empty() {
                // Initial value for jitter
                self.rtp_jitter = d;
            } else {
                self.rtp_jitter += (d - self.rtp_jitter) / 16.0;
            }

            self.rtp_jitter_history.push(self.rtp_jitter);
            if self.rtp_jitter_history.len() > 300 {
                self.rtp_jitter_history.remove(0);
            }
        }

        // Calculate statistics for RTT
        self.median = calculate_percentile(&valid_data, 50.0);
        self.p95 = calculate_percentile(&valid_data, 95.0);
        self.min_rtt = valid_data.iter().copied().fold(f64::INFINITY, f64::min);
        self.max_rtt = valid_data.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        let variance = valid_data
            .iter()
            .map(|&v| {
                let diff = v - self.mean;
                diff * diff
            })
            .sum::<f64>()
            / valid_data.len() as f64;
        self.stddev = variance.sqrt();

        // Calculate statistics for RTP Jitter history
        if !self.rtp_jitter_history.is_empty() {
            self.rtp_jitter_mean =
                self.rtp_jitter_history.iter().sum::<f64>() / self.rtp_jitter_history.len() as f64;
            self.rtp_jitter_median = calculate_percentile(&self.rtp_jitter_history, 50.0);
        }

        // Calculate Outliers
        let threshold = self.mean + 3.0 * self.stddev;
        if self.stddev > 0.1 {
            self.outliers = valid_data.iter().filter(|&&v| v > threshold).count() as u32;
        } else {
            self.outliers = 0;
        }

        // Calculate MOS
        let loss_pct = 100.0 - self.availability;
        self.mos = calculate_mos(self.mean, self.rtp_jitter, loss_pct);
    }

    /// Resets all statistics fields, typically used when stopping the host pinging.
    pub fn reset_statistics(&mut self) {
        self.alive = false;
        self.latency = f64::NAN;
        self.history.clear();
        self.mean = 0.0;
        self.rtp_jitter = 0.0;
        self.rtp_jitter_history.clear();
        self.median = 0.0;
        self.p95 = 0.0;
        self.stddev = 0.0;
        self.min_rtt = f64::INFINITY;
        self.max_rtt = f64::NEG_INFINITY;
        self.rtp_jitter_mean = 0.0;
        self.rtp_jitter_median = 0.0;
        self.mos = 0.0;
        self.availability = 0.0;
        self.outliers = 0;
        self.streak = 0;
        self.streak_success = false;
        self.sent = 0;
        self.lost = 0;
        self.prev_alive = None;
        self.incident_start = None;
        self.log_pings_since_stats = 0;
        self.events.clear();
        // Do not reset traceroute_path, tracking states for traceroute
    }
}

/// Calculates MOS (Mean Opinion Score) based on RTT, Jitter and Loss.
/// Range: 1.0 (Bad) to 4.5 (Excellent).
pub fn calculate_mos(rtt: f64, jitter: f64, loss_pct: f64) -> f64 {
    // Effective latency
    let effective_latency = rtt + jitter * 2.0 + 10.0;

    let r = if effective_latency < 160.0 {
        94.2 - effective_latency / 40.0
    } else {
        94.2 - (effective_latency - 120.0) / 10.0
    };

    // Damage from loss
    let r = r - (loss_pct * 2.5);

    // Limit R to [0, 100]
    let r = r.clamp(0.0, 100.0);

    // MOS calculation
    1.0 + 0.035 * r + 0.000007 * r * (r - 60.0) * (100.0 - r)
}

/// Calculates a percentile from a slice of data.
pub fn calculate_percentile(data: &[f64], percentile: f64) -> f64 {
    if data.is_empty() {
        return 0.0;
    }
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let pos = (percentile / 100.0) * (sorted.len() - 1) as f64;
    let base = pos.floor() as usize;
    let fract = pos - base as f64;
    if base + 1 < sorted.len() {
        sorted[base] + fract * (sorted[base + 1] - sorted[base])
    } else {
        sorted[base]
    }
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
