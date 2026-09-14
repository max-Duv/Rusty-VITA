use std::fs;
use std::process::Command;
use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct ProcessTelemetry {
    /// Process CPU as a percentage of total logical host capacity (0-100% on a healthy reading).
    pub cpu_percent: Option<f64>,
    /// Raw Linux process CPU usage in single-core equivalents. 100% == one fully occupied logical CPU.
    pub cpu_core_percent: Option<f64>,
    pub logical_cpus: usize,
    pub memory_bytes: Option<u64>,
}

#[derive(Debug)]
pub struct ProcessMonitor {
    clk_tck: Option<f64>,
    logical_cpus: usize,
    last_ticks: Option<u64>,
    last_sample: Instant,
    last: ProcessTelemetry,
}

impl Default for ProcessMonitor {
    fn default() -> Self { Self::new() }
}

impl ProcessMonitor {
    pub fn new() -> Self {
        let clk_tck = Command::new("getconf")
            .arg("CLK_TCK")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .and_then(|s| s.trim().parse::<f64>().ok())
            .filter(|v| *v > 0.0);
        let logical_cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).max(1);
        let last_ticks = read_process_ticks();
        Self {
            clk_tck,
            logical_cpus,
            last_ticks,
            last_sample: Instant::now(),
            last: ProcessTelemetry {
                cpu_percent: None,
                cpu_core_percent: None,
                logical_cpus,
                memory_bytes: read_rss_bytes(),
            },
        }
    }

    pub fn sample(&mut self) -> ProcessTelemetry {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_sample).as_secs_f64();
        let ticks = read_process_ticks();
        let core_percent = match (ticks, self.last_ticks, self.clk_tck) {
            (Some(cur), Some(prev), Some(hz)) if elapsed > 0.05 && cur >= prev => {
                Some((((cur - prev) as f64 / hz) / elapsed * 100.0).max(0.0))
            }
            _ => self.last.cpu_core_percent,
        };
        let capacity_percent = core_percent.map(|v| (v / self.logical_cpus as f64).clamp(0.0, 100.0));
        if elapsed >= 0.25 {
            self.last_ticks = ticks;
            self.last_sample = now;
        }
        self.last = ProcessTelemetry {
            cpu_percent: capacity_percent,
            cpu_core_percent: core_percent,
            logical_cpus: self.logical_cpus,
            memory_bytes: read_rss_bytes().or(self.last.memory_bytes),
        };
        self.last.clone()
    }
}

fn read_process_ticks() -> Option<u64> {
    let stat = fs::read_to_string("/proc/self/stat").ok()?;
    let close = stat.rfind(')')?;
    let rest = stat.get(close + 1..)?.trim();
    let fields: Vec<&str> = rest.split_whitespace().collect();
    // /proc/<pid>/stat fields 14 and 15 are utime/stime. `fields[0]` here is field 3.
    let utime = fields.get(11)?.parse::<u64>().ok()?;
    let stime = fields.get(12)?.parse::<u64>().ok()?;
    Some(utime.saturating_add(stime))
}

fn read_rss_bytes() -> Option<u64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb = rest.split_whitespace().next()?.parse::<u64>().ok()?;
            return Some(kb.saturating_mul(1024));
        }
    }
    None
}
