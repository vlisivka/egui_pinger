/// Size of the sliding window for RTT and jitter history.
pub const HISTORY_WINDOW_SIZE: usize = 300;

/// Maximum number of events stored per host in memory.
pub const MAX_EVENTS_PER_HOST: usize = 100_000;

/// Maximum events displayed in the UI log viewer.
pub const MAX_UI_EVENTS: usize = 10_000;

/// RTT threshold (ms) for the warning line on the chart.
pub const RTT_WARNING_THRESHOLD_MS: f64 = 150.0;

/// Number of consecutive failures/successes to confirm state change.
pub const STATE_CONFIRMATION_STREAK: u32 = 3;

/// Freshness timeout for hop data in failure deduction (seconds).
pub const HOP_DATA_FRESHNESS_SEC: u64 = 15;

/// Interval between periodic traceroutes (seconds).
pub const TRACEROUTE_INTERVAL_SEC: u64 = 3600;

/// Minimum cooldown before re-tracing after status change (seconds).
pub const TRACEROUTE_MIN_COOLDOWN_SEC: u64 = 60;

/// Periodic statistics snapshot interval (every N pings).
pub const STATS_SNAPSHOT_INTERVAL: u32 = 300;

/// RFC 3550 smoothing divisor for RTP jitter calculation.
pub const RTP_JITTER_SMOOTHING_DIVISOR: f64 = 16.0;
