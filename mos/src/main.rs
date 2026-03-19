use std::collections::VecDeque;
use std::io::{BufRead, Write};

/// Smoothing factor for RTP Jitter calculation as per RFC 3550.
/// Higher values make the jitter less sensitive to individual packet variations.
const RTP_JITTER_SMOOTHING_DIVISOR: f64 = 16.0;

/// Represents the classification of a single line from the ping output.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PingResult {
    /// A successful response with the Round Trip Time in milliseconds.
    Success(f64),
    /// A recognized error message (timeout, unreachable, etc.) indicating packet loss.
    Timeout,
    /// A line that doesn't look like a ping response or error (e.g., headers, summaries).
    Unknown,
}

/// Controls which statistical metrics are included in the periodic output block.
struct DisplaySettings {
    show_mean: bool,
    show_median: bool,
    show_p95: bool,
    show_rtp_jitter: bool,
    show_rtp_mean_jitter: bool,
    show_rtp_median_jitter: bool,
    show_mos: bool,
    show_loss: bool,
    show_availability: bool,
    show_outliers: bool,
    show_stddev: bool,
    show_min_max: bool,
    show_streak: bool,
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
struct MosStatus {
    /// Desired size for the sliding window of statistical calculations.
    window_size: usize,
    /// History of RTT values for the current window. Loss is represented by NaN.
    history: VecDeque<f64>,
    /// History of calculated RTP Jitter values over the window.
    rtp_jitter_history: VecDeque<f64>,
    /// Current smoothed RTP Jitter value.
    rtp_jitter: f64,

    /// Total packets sent during this session.
    sent: u32,
    /// Total packets lost during this session.
    lost: u32,
    /// Current number of consecutive packets of the same type (success or failure).
    streak: u32,
    /// Whether the current streak is a series of successful pings.
    streak_success: bool,

    // Calculated metrics for the current sliding window
    mean: f64,
    median: f64,
    p95: f64,
    stddev: f64,
    min_rtt: f64,
    max_rtt: f64,
    rtp_jitter_mean: f64,
    rtp_jitter_median: f64,
    outliers: u32,
    mos: f64,
    availability: f64,

    /// Tracks the last reported state to prevent redundant incident notifications.
    /// Some(true) means "lost" was last reported, Some(false) means "restored" was last reported.
    last_incident_reported: Option<bool>,
}

impl MosStatus {
    fn new(window_size: usize) -> Self {
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
    fn add_sample(&mut self, result: PingResult) -> Option<String> {
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
    fn recalculate_stats(&mut self) {
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
    fn calculate_stats(&self, display: &DisplaySettings) -> String {
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
fn calculate_percentile(data: &[f64], percentile: f64) -> f64 {
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
fn calculate_mos(rtt: f64, jitter: f64, loss_pct: f64) -> f64 {
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
fn has_ip(l: &str) -> bool {
    // Look for patterns like X.X.X.X or X:X:X
    let mut dots = 0;
    let mut digits = 0;
    for c in l.chars() {
        if c.is_ascii_digit() {
            digits += 1;
        } else if c == '.' {
            if digits > 0 {
                dots += 1;
            }
            digits = 0;
        } else if c == ':' {
            // Treat as potentially successful IPv6 detection if multiple colons are found
            if dots >= 3 {
                return true;
            }
            dots = 0;
            digits = 0;
            if l.contains(':') && l.chars().filter(|&c| c == ':').count() >= 2 {
                return true;
            }
        } else {
            // End of potential address segment
            if dots >= 3 && digits > 0 {
                return true;
            }
            dots = 0;
            digits = 0;
        }
    }
    dots >= 3 && digits > 0
}

/// Analyzes a raw line of text from the `ping` utility to extract its meaning.
/// Robust against various localizations and formats by searching for co-occurence of IP addresses
/// and time-related numeric values.
fn parse_line(line: &str) -> PingResult {
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

    // Packet loss detection: search for common error markers across languages.
    if l.contains("timeout")
        || l.contains("тайм-аут")
        || l.contains("тайм-ауту")
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
fn toggle_stat(display: &mut DisplaySettings, stat: &str, enable: bool) {
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

fn main() {
    // Collect and parse command line arguments using manual matching to avoid dependencies.
    let args: Vec<String> = std::env::args().collect();
    let mut args_slice = &args[1..];

    let mut window_size = 300;
    let mut display = DisplaySettings::default();

    // Loop through arguments using list-pattern matching (tail @ ..)
    while let [head, tail @ ..] = args_slice {
        match head.as_str() {
            "--help" | "-h" => {
                println!("mos: CLI utility for ping statistics calculation");
                println!("Usage: ping <host> | mos [OPTIONS]");
                println!("\nOptions:");
                println!(
                    "  -n, --number-of-lines NUM  Number of lines for statistics (default: 300)"
                );
                println!(
                    "  -e, --enable STAT          Enable field: mean|median|p95|jitter|jitter_mean|jitter_median|mos|loss|availability|outliers|stddev|minmax|streak"
                );
                println!(
                    "  -d, --disable STAT         Disable field: mean|median|p95|jitter|jitter_mean|jitter_median|mos|loss|availability|outliers|stddev|minmax|streak"
                );
                println!("  -h, --help                 Show this help message");
                return;
            }
            "-n" | "--number-of-lines" => {
                if let [num_str, rest @ ..] = tail {
                    if let Ok(num) = num_str.parse::<usize>() {
                        window_size = num;
                    }
                    args_slice = rest;
                } else {
                    eprintln!("Error: -n|--number-of-lines requires a numeric value");
                    std::process::exit(1);
                }
            }
            "-e" | "--enable" => {
                if let [stat, rest @ ..] = tail {
                    toggle_stat(&mut display, stat, true);
                    args_slice = rest;
                } else {
                    eprintln!("Error: -e|--enable requires a statistic name");
                    std::process::exit(1);
                }
            }
            "-d" | "--disable" => {
                if let [stat, rest @ ..] = tail {
                    toggle_stat(&mut display, stat, false);
                    args_slice = rest;
                } else {
                    eprintln!("Error: -d|--disable requires a statistic name");
                    std::process::exit(1);
                }
            }
            _ => {
                eprintln!("Error: Unknown argument: {}", head);
                std::process::exit(1);
            }
        }
    }

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut status = MosStatus::new(window_size);
    let mut count = 0;

    // Main stream processing loop: read stdin line by line and mirror to stdout.
    for line in stdin.lock().lines() {
        if let Ok(l) = line {
            let res = parse_line(&l);
            // DUPLICATE original ping output to the terminal or next pipe
            if let Err(e) = writeln!(stdout, "{}", l) {
                // Gracefully handle broken pipe (e.g., if used with | head)
                if e.kind() == std::io::ErrorKind::BrokenPipe {
                    break;
                }
            }

            // Update internal state and print notifications for connectivity changes
            if let Some(msg) = status.add_sample(res) {
                writeln!(stdout, "! {}", msg).unwrap();
            }

            // Track window progress and print the aggregate statistics block periodically
            if res != PingResult::Unknown {
                count += 1;
                if count >= window_size {
                    writeln!(stdout, "{}", status.calculate_stats(&display)).unwrap();
                    count = 0;
                }
            }
        }
    }

    // Print final statistics if the stream ends before a full window completes
    if count > 0 {
        writeln!(stdout, "{}", status.calculate_stats(&display)).unwrap();
    }
}
