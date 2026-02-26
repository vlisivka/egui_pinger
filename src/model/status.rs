use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PingMode {
    Fast, // 1 second
    Slow, // 1 minute
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
    pub show_rtp_jitter: bool,
    #[serde(default = "default_false")]
    pub show_jitter_t3: bool,
    #[serde(default = "default_false")]
    pub show_jitter_t21: bool,
    #[serde(default = "default_false")]
    pub show_jitter_t99: bool,
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
            show_rtp_jitter: true,
            show_jitter_t3: false,
            show_jitter_t21: false,
            show_jitter_t99: false,
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
}

fn default_ping_mode() -> PingMode {
    PingMode::Fast
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
    /// Arithmetic mean of latency
    #[serde(skip, default)]
    pub mean: f64,
    /// Jitter for the last 3 results (T3)
    #[serde(skip, default)]
    pub jitter_3: f64,
    /// Jitter for the last 21 results (T21)
    #[serde(skip, default)]
    pub jitter_21: f64,
    /// Average jitter for all samples (T99) - Statistical
    #[serde(skip, default)]
    pub jitter_99: f64,
    /// Standard RTP Jitter according to RFC 3550
    #[serde(skip, default)]
    pub rtp_jitter: f64,
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
        }

        self.latency = rtt_ms;

        // Add to history (maximum 99 samples)
        self.history.push(rtt_ms);
        if self.history.len() > 99 {
            self.history.remove(0);
        }

        let valid_data: Vec<f64> = self
            .history
            .iter()
            .copied()
            .filter(|v| !v.is_nan())
            .collect();

        if valid_data.len() < 2 {
            // Not enough data for jitter calculation
            self.mean = if valid_data.is_empty() {
                0.0
            } else {
                valid_data[0]
            };
            self.jitter_3 = 0.0;
            self.jitter_21 = 0.0;
            self.jitter_99 = 0.0;
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
        }

        // Calculate jitter for different windows (Statistical)
        self.jitter_99 = calculate_jitter(&valid_data);

        let start_index_21 = valid_data.len().saturating_sub(21);
        self.jitter_21 = calculate_jitter(&valid_data[start_index_21..]);

        let start_index_3 = valid_data.len().saturating_sub(3);
        self.jitter_3 = calculate_jitter(&valid_data[start_index_3..]);
    }
}

/// Calculates average jitter from a slice of RTT samples.
/// Jitter is calculated as the average absolute difference between consecutive samples.
pub fn calculate_jitter(valid_data: &[f64]) -> f64 {
    if valid_data.len() < 2 {
        return 0.0;
    }

    let mut total_diff = 0.0;
    for window in valid_data.windows(2) {
        let diff = (window[1] - window[0]).abs();
        total_diff += diff;
    }

    total_diff / (valid_data.len() - 1) as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_jitter_empty() {
        assert_eq!(calculate_jitter(&[]), 0.0);
    }

    #[test]
    fn test_calculate_jitter_single() {
        assert_eq!(calculate_jitter(&[10.0]), 0.0);
    }

    #[test]
    fn test_calculate_jitter_stable() {
        assert_eq!(calculate_jitter(&[10.0, 10.0, 10.0]), 0.0);
    }

    #[test]
    fn test_calculate_jitter_increasing() {
        // |20-10| + |30-20| = 10 + 10 = 20. 20 / (3-1) = 10.0
        assert_eq!(calculate_jitter(&[10.0, 20.0, 30.0]), 10.0);
    }

    #[test]
    fn test_add_sample_stats() {
        let mut status = HostStatus::default();
        status.add_sample(10.0);
        status.add_sample(20.0);
        status.add_sample(f64::NAN);

        assert_eq!(status.sent, 3);
        assert_eq!(status.lost, 1);
        assert_eq!(status.mean, 15.0); // (10+20)/2
        assert_eq!(status.jitter_99, 10.0); // |20-10|/1
    }

    #[test]
    fn test_history_limit() {
        let mut status = HostStatus::default();
        for i in 0..150 {
            status.add_sample(i as f64);
        }
        assert_eq!(status.history.len(), 99);
        assert_eq!(status.history[0], 51.0);
        assert_eq!(status.history[98], 149.0);
    }
}
