use std::collections::VecDeque;
use std::io::{BufRead, Write};

/// Smoothing factor for RTP Jitter calculation as per RFC 3550.
/// Higher values make the jitter less sensitive to individual packet variations.
pub const RTP_JITTER_SMOOTHING_DIVISOR: f64 = 16.0;

/// Represents the classification of a single line from the ping output.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PingResult {
    /// A successful response with the Round Trip Time in milliseconds.
    Success(f64),
    /// A recognized error message (timeout, unreachable, etc.) indicating packet loss.
    Timeout,
    /// A line that doesn't look like a ping response or error (e.g., headers, summaries).
    Unknown,
}

/// Controls which statistical metrics are included in the periodic output block.
#[derive(Debug, Clone)]
pub struct DisplaySettings {
    pub show_mean: bool,
    pub show_median: bool,
    pub show_p95: bool,
    pub show_rtp_jitter: bool,
    pub show_rtp_mean_jitter: bool,
    pub show_rtp_median_jitter: bool,
    pub show_mos: bool,
    pub show_loss: bool,
    pub show_availability: bool,
    pub show_outliers: bool,
    pub show_stddev: bool,
    pub show_min_max: bool,
    pub show_streak: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            show_mean: true,
            show_median: true,
            show_p95: true,
            show_rtp_jitter: true,
            show_rtp_mean_jitter: true,
            show_rtp_median_jitter: true,
            show_mos: true,
            show_loss: true,
            show_availability: true,
            show_outliers: true,
            show_stddev: true,
            show_min_max: true,
            show_streak: true,
        }
    }
}

/// Core state for tracking network performance and calculating statistics over time.
pub struct MosStatus {
    /// Desired size for the sliding window of statistical calculations.
    pub window_size: usize,
    /// History of RTT values for the current window. Loss is represented by NaN.
    pub history: VecDeque<f64>,
    /// History of calculated RTP Jitter values over the window.
    pub rtp_jitter_history: VecDeque<f64>,
    /// Current smoothed RTP Jitter value.
    pub rtp_jitter: f64,

    /// Total packets sent during this session.
    pub sent: u32,
    /// Total packets lost during this session.
    pub lost: u32,
    /// Current number of consecutive packets of the same type (success or failure).
    pub streak: u32,
    /// Whether the current streak is a series of successful pings.
    pub streak_success: bool,

    // Calculated metrics for the current sliding window
    pub mean: f64,
    pub median: f64,
    pub p95: f64,
    pub stddev: f64,
    pub min_rtt: f64,
    pub max_rtt: f64,
    pub rtp_jitter_mean: f64,
    pub rtp_jitter_median: f64,
    pub outliers: u32,
    pub mos: f64,
    pub availability: f64,

    /// Tracks the last reported state to prevent redundant incident notifications.
    /// Some(true) means "lost" was last reported, Some(false) means "restored" was last reported.
    pub last_incident_reported: Option<bool>,
}

impl MosStatus {
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            history: VecDeque::with_capacity(window_size),
            rtp_jitter_history: VecDeque::with_capacity(window_size),
            rtp_jitter: 0.0,
            sent: 0,
            lost: 0,
            streak: 0,
            streak_success: true,
            mean: 0.0,
            median: 0.0,
            p95: 0.0,
            stddev: 0.0,
            min_rtt: f64::INFINITY,
            max_rtt: f64::NEG_INFINITY,
            rtp_jitter_mean: 0.0,
            rtp_jitter_median: 0.0,
            outliers: 0,
            mos: 0.0,
            availability: 100.0,
            last_incident_reported: None,
        }
    }

    /// Primary entry point for updating state with a new ping result.
    /// Manages incident detection (3+ consecutive losses) and jitter smoothing.
    pub fn add_sample(&mut self, result: PingResult) -> Option<String> {
        let mut incident_msg = None;

        match result {
            PingResult::Success(rtt_ms) => {
                self.sent += 1;
                // Transition logic for switching between success/failure streaks
                if self.streak_success {
                    self.streak += 1;
                } else {
                    // Check if a previously reported "lost" state is now resolved
                    if self.streak >= 3 && self.last_incident_reported == Some(true) {
                        incident_msg = Some(format!(
                            "Connectivity restored, {} pings missed",
                            self.streak
                        ));
                        self.last_incident_reported = Some(false);
                    }
                    self.streak = 1;
                    self.streak_success = true;
                }

                self.history.push_back(rtt_ms);

                // Calculate jitter based on the difference between the current and previous successful RTT.
                // We use historical data in the current window to find the most recent successful response.
                if self.history.len() >= 2 {
                    let mut prev_rtt_opt = None;
                    for h in self.history.iter().rev().skip(1) {
                        if !h.is_nan() {
                            prev_rtt_opt = Some(*h);
                            break;
                        }
                    }

                    if let Some(prev_rtt) = prev_rtt_opt {
                        let d = (rtt_ms - prev_rtt).abs();
                        // Iterative smoothing: J = J + (|D| - J) / 16
                        if self.rtp_jitter_history.is_empty() {
                            self.rtp_jitter = d;
                        } else {
                            self.rtp_jitter += (d - self.rtp_jitter) / RTP_JITTER_SMOOTHING_DIVISOR;
                        }
                        self.rtp_jitter_history.push_back(self.rtp_jitter);
                    }
                }
            }
            PingResult::Timeout => {
                self.sent += 1;
                self.lost += 1;
                // Transition logic for switching between success/failure streaks
                if !self.streak_success {
                    self.streak += 1;
                    // Incident detection: report "lost" after 3 consecutive failures
                    if self.streak == 3 && self.last_incident_reported != Some(true) {
                        incident_msg = Some("Connectivity lost".to_string());
                        self.last_incident_reported = Some(true);
                    }
                } else {
                    self.streak = 1;
                    self.streak_success = false;
                }
                // Mark loss in history window
                self.history.push_back(f64::NAN);
            }
            PingResult::Unknown => {
                // Ignore lines that aren't clear responses or errors
                return None;
            }
        }

        // Maintain the sliding window size
        if self.history.len() > self.window_size {
            self.history.pop_front();
        }
        if self.rtp_jitter_history.len() > self.window_size {
            self.rtp_jitter_history.pop_front();
        }

        // Recompute window-based metrics immediately for accurate reporting
        self.recalculate_stats();
        incident_msg
    }

    /// Performs the heavy lifting of statistical calculations for the current window.
    /// Handles Mean, Median, P95, StdDev, Min/Max, and MOS based on successful pings.
    pub fn recalculate_stats(&mut self) {
        let total = self.history.len();
        if total == 0 {
            return;
        }

        // Window-specific availability and loss
        let lost_in_window = self.history.iter().filter(|v| v.is_nan()).count();
        self.availability = (total - lost_in_window) as f64 / total as f64 * 100.0;
        let loss_pct = 100.0 - self.availability;

        // Isolate successful samples for RTT-specific calculations
        let valid_data: Vec<f64> = self
            .history
            .iter()
            .copied()
            .filter(|v| !v.is_nan())
            .collect();

        if valid_data.is_empty() {
            // Null state if no successful pings in the current window
            self.mean = 0.0;
            self.median = 0.0;
            self.p95 = 0.0;
            self.stddev = 0.0;
            self.min_rtt = f64::INFINITY;
            self.max_rtt = f64::NEG_INFINITY;
            self.rtp_jitter_mean = 0.0;
            self.rtp_jitter_median = 0.0;
            self.outliers = 0;
            self.mos = calculate_mos(0.0, 0.0, loss_pct);
            return;
        }

        // Basic arithmetic mean and range
        self.mean = valid_data.iter().sum::<f64>() / valid_data.len() as f64;
        self.median = calculate_percentile(&valid_data, 50.0);
        self.p95 = calculate_percentile(&valid_data, 95.0);
        self.min_rtt = valid_data.iter().copied().fold(f64::INFINITY, f64::min);
        self.max_rtt = valid_data.iter().copied().fold(f64::NEG_INFINITY, f64::max);

        // Standard Deviation (square root of variance)
        let variance = valid_data
            .iter()
            .map(|&v| {
                let diff = v - self.mean;
                diff * diff
            })
            .sum::<f64>()
            / valid_data.len() as f64;
        self.stddev = variance.sqrt();

        // Historical Jitter metrics
        if !self.rtp_jitter_history.is_empty() {
            self.rtp_jitter_mean =
                self.rtp_jitter_history.iter().sum::<f64>() / self.rtp_jitter_history.len() as f64;
            let sorted_jitter = self
                .rtp_jitter_history
                .iter()
                .copied()
                .collect::<Vec<f64>>();
            self.rtp_jitter_median = calculate_percentile(&sorted_jitter, 50.0);
        }

        // Outlier detection: values exceeding Mean + 3 StdDevs (using a minimal noise threshold)
        let threshold = self.mean + 3.0 * self.stddev;
        if self.stddev > 0.1 {
            self.outliers = valid_data.iter().filter(|&&v| v > threshold).count() as u32;
        } else {
            self.outliers = 0;
        }

        // Perceptual quality score
        self.mos = calculate_mos(self.mean, self.rtp_jitter, loss_pct);
    }

    /// Formats the current metrics into a standardized string block for stdout.
    /// Respects the enabled/disabled flags for each metric.
    pub fn calculate_stats(&self, display: &DisplaySettings) -> String {
        let mut parts = Vec::new();

        if display.show_loss {
            parts.push(format!(
                "L:{:.1}% ({}/{})",
                100.0 - self.availability,
                self.lost,
                self.sent
            ));
        }
        if display.show_availability {
            parts.push(format!("Av:{:.0}%", self.availability));
        }

        if display.show_mean {
            parts.push(format!("M={:.1}ms", self.mean));
        }
        if display.show_median {
            parts.push(format!("Med={:.1}ms", self.median));
        }
        if display.show_rtp_jitter {
            parts.push(format!("J={:.1}ms", self.rtp_jitter));
        }
        if display.show_rtp_mean_jitter {
            parts.push(format!("Jm={:.1}ms", self.rtp_jitter_mean));
        }
        if display.show_rtp_median_jitter {
            parts.push(format!("Jmed={:.1}ms", self.rtp_jitter_median));
        }
        if display.show_mos {
            parts.push(format!("MOS={:.1}", self.mos));
        }
        if display.show_stddev {
            parts.push(format!("SD={:.1}", self.stddev));
        }
        if display.show_outliers {
            parts.push(format!("Out={}", self.outliers));
        }
        if display.show_min_max {
            parts.push(format!("m/M={:.0}/{:.0}", self.min_rtt, self.max_rtt));
        }
        if display.show_p95 {
            parts.push(format!("95%={:.1}ms", self.p95));
        }
        if display.show_streak {
            parts.push(format!("Str={}", self.streak));
        }

        format!("# Statistics: {}", parts.join(" "))
    }
}

/// Generic percentile calculation using sorting and linear interpolation.
pub fn calculate_percentile(data: &[f64], percentile: f64) -> f64 {
    let mut sorted = data.to_vec();
    if sorted.is_empty() {
        return 0.0;
    }

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

/// Calculates the MOS (Mean Opinion Score) for voice calls based on RTT, Jitter, and Loss.
/// Scales from 1.0 (unusable) to ~4.5 (excellent).
pub fn calculate_mos(rtt: f64, jitter: f64, loss_pct: f64) -> f64 {
    let effective_latency = rtt + jitter * 2.0 + 10.0;
    let r = if effective_latency < 160.0 {
        94.2 - effective_latency / 40.0
    } else {
        94.2 - (effective_latency - 120.0) / 10.0
    };
    let r = (r - (loss_pct * 2.5)).clamp(0.0, 100.0);
    1.0 + 0.035 * r + 0.000007 * r * (r - 60.0) * (100.0 - r)
}

/// Heuristic-based IP address detection (IPv4 or IPv6) without using large dependencies like `regex`.
/// Counts standard separators (dots for IPv4, colons for IPv6) within numeric clusters.
pub fn has_ip(l: &str) -> bool {
    let mut dots = 0;
    let mut digits = 0;
    let mut colons = 0;

    for &b in l.as_bytes() {
        match b {
            b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F' => {
                digits += 1;
            }
            b'.' => {
                if digits > 0 {
                    dots += 1;
                }
                digits = 0;
            }
            b':' => {
                // If we found a colon after 3 dots, it's likely an IPv4 address followed by a colon
                if dots >= 3 && digits > 0 {
                    return true;
                }

                colons += 1;
                dots = 0;
                digits = 0;

                // Typical IPv6 has at least 2 colons
                if colons >= 2 {
                    return true;
                }
            }
            _ => {
                // Segment ended
                if dots >= 3 && digits > 0 {
                    return true;
                }
                dots = 0;
                digits = 0;
            }
        }
    }
    // Final check for the last segment
    dots >= 3 && digits > 0
}

/// Analyzes a raw line of text from the `ping` utility to extract its meaning.
/// Robust against various localizations and formats by searching for co-occurence of IP addresses
/// and time-related numeric values.
pub fn parse_line(line: &str) -> PingResult {
    let l = line.to_lowercase();

    let has_ip_addr = has_ip(&l);
    // Common localizations for "time" in various languages
    let has_time = l.contains("time") || l.contains("час");

    if has_ip_addr && has_time {
        // Success case: line contains both an address and a reported duration.
        // Extraction logic handles various formats: "time=13.2 ms", "час <1мс", "time 13ms".
        for keyword in &["time", "час"] {
            if let Some(pos) = l.find(keyword) {
                let rest = &l[pos + keyword.len()..];
                let val_str = rest
                    .chars()
                    .skip_while(|c| !c.is_numeric() && *c != '.')
                    .take_while(|c| c.is_numeric() || *c == '.')
                    .collect::<String>();
                if !val_str.is_empty() {
                    if let Ok(val) = val_str.parse::<f64>() {
                        return PingResult::Success(val);
                    }
                }
                // Handle Windows-style "<1ms" precision limit
                if rest.contains("<") {
                    return PingResult::Success(1.0);
                }
            }
        }
    }

    // Packet loss detection: search for common error markers (English and Ukrainian).
    if l.contains("timeout")
        || l.contains("тайм-аут")
        || l.contains("no answer")
        || l.contains("очікування")
        || l.contains("unreachable")
        || l.contains("недосяжний")
        || l.contains("expired")
        || l.contains("перевищено")
    {
        return PingResult::Timeout;
    }

    // Unrecognized line (headers, summary totals, or random system messages)
    PingResult::Unknown
}

/// Helper mapping for turning command-line STAT names into DisplaySettings toggles.
pub fn toggle_stat(display: &mut DisplaySettings, stat: &str, enable: bool) {
    match stat.to_lowercase().as_str() {
        "mean" | "m" => display.show_mean = enable,
        "median" | "med" => display.show_median = enable,
        "p95" | "95%" => display.show_p95 = enable,
        "jitter" | "j" => display.show_rtp_jitter = enable,
        "jitter_mean" | "jm" => display.show_rtp_mean_jitter = enable,
        "jitter_median" | "jmed" => display.show_rtp_median_jitter = enable,
        "mos" => display.show_mos = enable,
        "loss" | "l" => display.show_loss = enable,
        "availability" | "av" => display.show_availability = enable,
        "outliers" | "out" => display.show_outliers = enable,
        "stddev" | "sd" => display.show_stddev = enable,
        "minmax" | "mm" => display.show_min_max = enable,
        "streak" | "str" => display.show_streak = enable,
        _ => eprintln!("Warning: Unknown statistic: {}", stat),
    }
}

/// Runs the main loop: reading from reader, parsing, updating status, and writing results.
pub fn run_loop<R: BufRead, W: Write>(
    mut reader: R,
    mut writer: W,
    window_size: usize,
    display: &DisplaySettings,
) -> std::io::Result<()> {
    let mut status = MosStatus::new(window_size);
    let mut count = 0;
    let mut line = String::new();

    while reader.read_line(&mut line)? > 0 {
        let l = line.trim_end();
        let res = parse_line(l);
        
        // DUPLICATE original ping output
        if let Err(e) = writeln!(writer, "{}", l) {
            if e.kind() == std::io::ErrorKind::BrokenPipe {
                break;
            }
            return Err(e);
        }

        // Update internal state and print notifications
        if let Some(msg) = status.add_sample(res) {
            writeln!(writer, "! {}", msg)?;
        }

        // Periodic statistics reporting
        if res != PingResult::Unknown {
            count += 1;
            if count >= window_size {
                writeln!(writer, "{}", status.calculate_stats(display))?;
                count = 0;
            }
        }
        line.clear();
    }

    // Final statistics
    if count > 0 {
        writeln!(writer, "{}", status.calculate_stats(display))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_parse_line_linux_en() {
        let line = "64 bytes from 8.8.8.8: icmp_seq=1 ttl=117 time=13.2 ms";
        assert_eq!(parse_line(line), PingResult::Success(13.2));
    }

    #[test]
    fn test_parse_line_linux_ua() {
        let line = "64 байтів від 8.8.8.8: icmp_seq=1 ttl=117 час=13.2 мс";
        assert_eq!(parse_line(line), PingResult::Success(13.2));
    }

    #[test]
    fn test_parse_line_windows_ua() {
        let line = "Відповідь від 8.8.8.8: число байтів=32 час=13мс TTL=117";
        assert_eq!(parse_line(line), PingResult::Success(13.0));
    }

    #[test]
    fn test_parse_line_windows_precision() {
        let line = "Відповідь від 8.8.8.8: число байтів=32 час <1мс TTL=117";
        assert_eq!(parse_line(line), PingResult::Success(1.0));
    }

    #[test]
    fn test_parse_line_timeout() {
        assert_eq!(
            parse_line("Request timeout for icmp_seq 2"),
            PingResult::Timeout
        );
        assert_eq!(parse_line("час очікування вичерпано."), PingResult::Timeout);
    }

    #[test]
    fn test_parse_line_unknown() {
        assert_eq!(
            parse_line("PING 8.8.8.8 (8.8.8.8): 56 data bytes"),
            PingResult::Unknown
        );
        assert_eq!(
            parse_line("--- 8.8.8.8 ping statistics ---"),
            PingResult::Unknown
        );
    }

    #[test]
    fn test_has_ip() {
        assert!(has_ip("8.8.8.8"));
        assert!(has_ip("from 1.2.3.4:"));
        assert!(has_ip("2001:4860:4860::8888"));
        assert!(has_ip("::1"));
        assert!(has_ip("fe80::1%lo0"));
        assert!(!has_ip("no ip here"));
        assert!(!has_ip("56(84) bytes"));
    }

    #[test]
    fn test_percentile() {
        let data = vec![10.0, 20.0, 30.0];
        assert_eq!(calculate_percentile(&data, 50.0), 20.0);
        assert_eq!(calculate_percentile(&data, 0.0), 10.0);
        assert_eq!(calculate_percentile(&data, 100.0), 30.0);

        let empty: Vec<f64> = vec![];
        assert_eq!(calculate_percentile(&empty, 50.0), 0.0);
    }

    #[test]
    fn test_mos() {
        // Perfect
        let m = calculate_mos(10.0, 0.0, 0.0);
        assert!(m > 4.0);

        // Terrible
        let m2 = calculate_mos(500.0, 50.0, 10.0);
        assert!(m2 < 2.0);
    }

    #[test]
    fn test_incident_streak() {
        let mut status = MosStatus::new(10);
        assert_eq!(status.add_sample(PingResult::Timeout), None); // 1
        assert_eq!(status.add_sample(PingResult::Timeout), None); // 2
        assert_eq!(
            status.add_sample(PingResult::Timeout),
            Some("Connectivity lost".to_string())
        ); // 3
        assert_eq!(status.add_sample(PingResult::Timeout), None); // 4

        assert_eq!(
            status.add_sample(PingResult::Success(10.0)),
            Some("Connectivity restored, 4 pings missed".to_string())
        );
    }

    #[test]
    fn test_calculate_stats_full() {
        let mut status = MosStatus::new(5);
        status.add_sample(PingResult::Success(10.0));
        status.recalculate_stats();
        let stats = status.calculate_stats(&DisplaySettings::default());
        assert!(stats.contains("M=10.0ms"));
        assert!(stats.contains("Av:100%"));
        assert!(stats.contains("Str=1"));
        assert!(stats.contains("m/M=10/10"));
    }

    #[test]
    fn test_run_loop() {
        let input = "64 bytes from 8.8.8.8: time=10.0 ms\n".repeat(3);
        let mut output = Vec::new();
        run_loop(Cursor::new(input), &mut output, 3, &DisplaySettings::default()).unwrap();
        let out_str = String::from_utf8(output).unwrap();
        assert!(out_str.contains("# Statistics:"));
        assert!(out_str.contains("M=10.0ms"));
    }

    #[test]
    fn test_toggle_stat() {
        let mut settings = DisplaySettings::default();
        toggle_stat(&mut settings, "mos", false);
        assert!(!settings.show_mos);
        toggle_stat(&mut settings, "m", false);
        assert!(!settings.show_mean);
    }
}
