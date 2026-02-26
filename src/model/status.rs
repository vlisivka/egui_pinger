use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostInfo {
    pub name: String,
    pub address: String,
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
    /// Average jitter for all samples (T99)
    #[serde(skip, default)]
    pub jitter_99: f64,
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

        // Calculate jitter for different windows
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
