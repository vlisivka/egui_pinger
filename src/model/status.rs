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
        } else {
            if self.streak_success {
                self.streak += 1;
            } else {
                self.streak = 1;
                self.streak_success = true;
            }
        }

        self.latency = rtt_ms;

        // Add to history (maximum 99 samples)
        self.history.push(rtt_ms);
        if self.history.len() > 99 {
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
            if self.rtp_jitter_history.len() > 99 {
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
        let loss_pct =
            (self.lost as f64 / if self.sent == 0 { 1 } else { self.sent } as f64) * 100.0;
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
mod tests {
    use super::*;

    #[test]
    fn test_add_sample_stats() {
        let mut status = HostStatus::default();
        status.add_sample(10.0);
        status.add_sample(20.0);
        status.add_sample(f64::NAN);

        assert_eq!(status.sent, 3);
        assert_eq!(status.lost, 1);
        assert_eq!(status.mean, 15.0); // (10+20)/2
        assert_eq!(status.availability, (2.0 / 3.0) * 100.0);
        assert_eq!(status.streak, 1);
        assert_eq!(status.streak_success, false); // Last was NaN
    }

    #[test]
    fn test_calculate_percentile() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(calculate_percentile(&data, 0.0), 1.0);
        assert_eq!(calculate_percentile(&data, 50.0), 3.0);
        assert_eq!(calculate_percentile(&data, 100.0), 5.0);
        assert_eq!(calculate_percentile(&data, 25.0), 2.0); // (0.25 * 4) = 1.0 -> idx 1 -> 2.0

        let data2 = vec![10.0, 20.0];
        assert_eq!(calculate_percentile(&data2, 50.0), 15.0); // interpolation
    }

    #[test]
    fn test_calculate_mos_values() {
        // Ideal network: Low RTT, no jitter, no loss
        let excellent = calculate_mos(10.0, 0.0, 0.0);
        assert!(excellent > 4.4);

        // Typical good network: 50ms RTT, 5ms jitter, 0% loss
        let good = calculate_mos(50.0, 5.0, 0.0);
        assert!(good > 4.0 && good < 4.4);

        // Degraded network: 150ms RTT, 20ms jitter, 1% loss
        let stressed = calculate_mos(150.0, 20.0, 1.0);
        // Effective latency 200ms -> R ~ 83.7 -> MOS ~ 4.1
        assert!(stressed < 4.2 && stressed > 3.0);

        // Bad network: 300ms RTT, 50ms jitter, 5% loss
        let bad = calculate_mos(300.0, 50.0, 5.0);
        assert!(bad < 3.0);
    }

    #[test]
    fn test_streaks() {
        let mut status = HostStatus::default();

        // Success streak
        status.add_sample(10.0);
        status.add_sample(10.0);
        status.add_sample(10.0);
        assert_eq!(status.streak, 3);
        assert_eq!(status.streak_success, true);

        // Switch to fail streak
        status.add_sample(f64::NAN);
        assert_eq!(status.streak, 1);
        assert_eq!(status.streak_success, false);

        status.add_sample(f64::NAN);
        assert_eq!(status.streak, 2);
        assert_eq!(status.streak_success, false);

        // Switch back to success
        status.add_sample(10.0);
        assert_eq!(status.streak, 1);
        assert_eq!(status.streak_success, true);
    }

    #[test]
    fn test_outliers_detection() {
        let mut status = HostStatus::default();
        // Establish stable baseline
        for _ in 0..10 {
            status.add_sample(10.0);
        }
        assert_eq!(status.outliers, 0);
        assert!(status.stddev < 0.1);

        // Add some variation to make stddev > 0.1
        status.add_sample(11.0);
        status.add_sample(9.0);

        // Threshold is mean + 3*std
        // Initially stddev=0, then we add 11.0.
        // With 10 samples of 10.0 and one 11.0, stddev is small enough that 11.0 might be an outlier.
        // Let's check status.outliers after the spike.
        status.add_sample(100.0);
        assert!(status.outliers >= 1);

        let prev_outliers = status.outliers;
        // Another normal sample
        status.add_sample(10.1);
        assert_eq!(status.outliers, prev_outliers);
    }

    #[test]
    fn test_advanced_stats() {
        let mut status = HostStatus::default();
        for &rtt in &[10.0, 20.0, 30.0, 40.0, 50.0] {
            status.add_sample(rtt);
        }

        assert_eq!(status.min_rtt, 10.0);
        assert_eq!(status.max_rtt, 50.0);
        assert_eq!(status.median, 30.0);
        assert!(status.p95 > 40.0);
        assert_eq!(status.mean, 30.0);
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
