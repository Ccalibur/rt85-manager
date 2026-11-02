//! Host readings for the screen's CPU and temperature widgets.

/// CPU usage from `/proc/stat`, as a delta between two samples.
#[derive(Default)]
pub struct CpuSampler {
    prev: Option<(u64, u64)>,
}

impl CpuSampler {
    /// Percent busy since the previous call (`None` on the first call).
    pub fn sample(&mut self) -> Option<u8> {
        let stat = std::fs::read_to_string("/proc/stat").ok()?;
        let v: Vec<u64> = stat.lines().next()?.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
        let idle = v.get(3)? + v.get(4).unwrap_or(&0);
        let total: u64 = v.iter().take(8).sum();
        let prev = self.prev.replace((idle, total));
        let (pi, pt) = prev?;
        let (di, dt) = (idle.saturating_sub(pi), total.saturating_sub(pt));
        (dt > 0).then(|| (100 * (dt - di) / dt).min(100) as u8)
    }
}

/// Hottest thermal zone, in °C.
pub fn temperature() -> Option<u8> {
    std::fs::read_dir("/sys/class/thermal")
        .ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("thermal_zone"))
        .filter_map(|e| std::fs::read_to_string(e.path().join("temp")).ok())
        .filter_map(|s| s.trim().parse::<i64>().ok())
        .max()
        .map(|milli| (milli / 1000).clamp(0, 99) as u8)
}
