//! Best-effort local macOS workload telemetry, using built-in system utilities.
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio::{process::Command, sync::Mutex};

#[derive(Default)]
pub struct Metrics(Mutex<Option<(Instant, Value)>>);
impl Metrics {
    pub async fn sample(&self, engine_pid: Option<u32>) -> Value {
        if !cfg!(target_os = "macos") {
            return Value::Null;
        }
        let mut cached = self.0.lock().await;
        if let Some((at, value)) = &*cached {
            if at.elapsed() < Duration::from_secs(2) {
                return value.clone();
            }
        }
        let value = sample_mac(engine_pid).await;
        *cached = Some((Instant::now(), value.clone()));
        value
    }
}
async fn output(command: &str, args: &[&str]) -> String {
    let mut cmd = Command::new(command);
    cmd.args(args).kill_on_drop(true);
    match tokio::time::timeout(Duration::from_secs(1), cmd.output()).await {
        Ok(Ok(out)) if out.status.success() => String::from_utf8_lossy(&out.stdout).into_owned(),
        _ => String::new(),
    }
}
fn number_after(text: &str, key: &str) -> Option<u64> {
    let tail = text.split_once(key)?.1.trim_start();
    let digits: String = tail.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}
fn swap_used(text: &str) -> Option<u64> {
    let tail = text.split_once("used =")?.1.trim_start();
    let digits: String = tail
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let multiplier = match tail.as_bytes().get(digits.len())? {
        b'K' => 1024.,
        b'M' => 1024_f64.powi(2),
        b'G' => 1024_f64.powi(3),
        _ => return None,
    };
    Some((digits.parse::<f64>().ok()? * multiplier) as u64)
}
async fn sample_mac(engine_pid: Option<u32>) -> Value {
    let (sys, vm, gpu) = tokio::join!(
        output(
            "/usr/sbin/sysctl",
            &[
                "-n",
                "hw.memsize",
                "kern.memorystatus_vm_pressure_level",
                "vm.swapusage"
            ]
        ),
        output("/usr/bin/vm_stat", &[]),
        output("/usr/sbin/ioreg", &["-r", "-c", "AGXAccelerator", "-l"]),
    );
    let lines: Vec<_> = sys.lines().collect();
    let total = lines.first().and_then(|v| v.parse::<u64>().ok());
    let pressure = match lines.get(1).copied() {
        Some("1") => Some("normal"),
        Some("2") => Some("warning"),
        Some("4") => Some("critical"),
        _ => None,
    };
    let page = number_after(&vm, "page size of ");
    let pages = |key: &str| number_after(&vm, key);
    // Exclude reclaimable file-backed and purgeable pages from memory in use.
    let used = (|| {
        Some(
            (pages("Pages active:")?
                + pages("Pages inactive:")?
                + pages("Pages wired down:")?
                + pages("Pages occupied by compressor:")?)
            .saturating_sub(pages("Pages purgeable:")? + pages("File-backed pages:")?)
                * page?,
        )
    })();
    let compressed = pages("Pages occupied by compressor:")
        .zip(page)
        .map(|(n, p)| n * p);
    let stats = gpu
        .split_once("\"PerformanceStatistics\" = {")
        .map(|(_, s)| s.split('}').next().unwrap_or(""));
    let utilization = stats
        .and_then(|s| number_after(s, "\"Device Utilization %\"="))
        .filter(|n| *n <= 100);
    let gpu_memory = stats.and_then(|s| number_after(s, "\"In use system memory\"="));
    let name = gpu
        .split_once("\"model\" = \"")
        .and_then(|(_, s)| s.split('"').next());
    let engine_rss = if let Some(pid) = engine_pid {
        output("/bin/ps", &["-o", "rss=", "-p", &pid.to_string()])
            .await
            .trim()
            .parse::<u64>()
            .ok()
            .map(|k| k * 1024)
    } else {
        None
    };
    json!({"gpu_name":name,"gpu_utilization":utilization,"gpu_memory_bytes":gpu_memory,"memory_total_bytes":total,"memory_used_bytes":used,"memory_pressure":pressure,"compressed_bytes":compressed,"swap_used_bytes":swap_used(&sys),"engine_rss_bytes":engine_rss})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_native_metric_units_and_missing_values() {
        assert_eq!(
            number_after("Pages active: 12345.\n", "Pages active:"),
            Some(12345)
        );
        assert_eq!(
            number_after("\"Device Utilization %\"=87,", "\"Device Utilization %\"="),
            Some(87)
        );
        assert_eq!(number_after("", "missing"), None);
        assert_eq!(
            swap_used("total = 3072.00M  used = 1852.25M free = 1M"),
            Some(1_942_224_896)
        );
    }
}
