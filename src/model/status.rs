use serde::{Deserialize, Serialize};

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
}

impl HostStatus {
    /// Adds a new RTT sample and updates statistics.
    pub fn add_sample(&mut self, rtt_ms: f64) {
        self.sent += 1;

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

        self.availability = (self.sent - self.lost) as f64 / self.sent as f64 * 100.0;

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
            self.mos = calculate_mos(self.mean, self.rtp_jitter, 0.0);
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

            if self.sent == 2 {
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
        if !rtt_ms.is_nan() && rtt_ms > threshold && self.stddev > 0.1 {
            self.outliers += 1;
        }

        // Calculate MOS
        let loss_pct = (self.lost as f64 / self.sent as f64) * 100.0;
        self.mos = calculate_mos(self.mean, self.rtp_jitter, loss_pct);
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
