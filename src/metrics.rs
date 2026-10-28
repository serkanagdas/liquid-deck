//! Live hardware metrics.
//!
//! The sampler is adaptive: while no dashboard is connected (and no HTTP
//! client asked for metrics recently) it only wakes up every few seconds and
//! skips all sysinfo refreshes, so an idle deck costs essentially no CPU.

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::Serialize;
use sysinfo::{Components, Networks, System};
use tokio::sync::{broadcast, Mutex, Notify, RwLock};

/// Number of one-second samples kept for the network sparkline.
pub const HISTORY_LEN: usize = 60;
/// Sentinel stored in the ping atomic when the last probe failed.
pub const PING_NONE: u32 = u32::MAX;

#[derive(Debug, Clone, Serialize)]
pub struct Metrics {
    /// Unix timestamp in milliseconds.
    pub ts: u64,
    /// Global CPU usage, percent.
    pub cpu: f32,
    /// Logical core count.
    pub cpu_cores: usize,
    /// Used RAM in bytes.
    pub ram_used: u64,
    /// Total RAM in bytes.
    pub ram_total: u64,
    /// Used RAM, percent.
    pub ram_pct: f32,
    /// Hottest readable sensor, Celsius. `null` when unavailable.
    pub temp: Option<f32>,
    /// Download rate, bytes per second.
    pub net_rx: f64,
    /// Upload rate, bytes per second.
    pub net_tx: f64,
    /// Total bytes received since boot (non-loopback).
    pub net_rx_total: u64,
    /// Total bytes sent since boot (non-loopback).
    pub net_tx_total: u64,
    /// TCP handshake latency in milliseconds. `null` when unreachable.
    pub ping: Option<f32>,
    /// System uptime in seconds.
    pub uptime: u64,
    /// 1 / 5 / 15 minute load average.
    pub load: [f32; 3],
    pub host: String,
    pub os: String,
    /// Interface carrying the most traffic.
    pub iface: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryPoint {
    pub t: u64,
    pub rx: f64,
    pub tx: f64,
    pub cpu: f32,
    pub ram: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct SamplerConfig {
    pub sample_ms: u64,
    pub idle_ms: u64,
}

#[derive(Serialize)]
struct MetricsEnvelope<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(flatten)]
    metrics: &'a Metrics,
}

#[allow(clippy::too_many_arguments)]
pub fn spawn_sampler(
    tx: broadcast::Sender<Arc<String>>,
    latest: Arc<RwLock<Option<Metrics>>>,
    history: Arc<Mutex<VecDeque<HistoryPoint>>>,
    clients: Arc<AtomicUsize>,
    activity: Arc<AtomicU64>,
    wake: Arc<Notify>,
    ping_us: Arc<AtomicU32>,
    config: SamplerConfig,
) {
    tokio::spawn(async move {
        let mut system = System::new();
        system.refresh_cpu_all();
        system.refresh_memory();

        let mut networks = Networks::new_with_refreshed_list();
        let mut components = Components::new_with_refreshed_list();

        let host = System::host_name().unwrap_or_else(|| "unknown".to_string());
        let os = System::long_os_version().unwrap_or_else(|| std::env::consts::OS.to_string());
        let cores = system.cpus().len().max(1);

        let mut last_net_refresh = Instant::now();
        let mut first = true;

        loop {
            if first {
                // Give the CPU counters a short window to produce a real value.
                tokio::time::sleep(Duration::from_millis(250)).await;
                first = false;
            } else {
                let active = clients.load(Ordering::Relaxed) > 0
                    || now_secs().saturating_sub(activity.load(Ordering::Relaxed)) < 10;

                if active {
                    tokio::time::sleep(Duration::from_millis(config.sample_ms)).await;
                } else {
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_millis(config.idle_ms)) => {}
                        _ = wake.notified() => {}
                    }
                }
            }

            system.refresh_cpu_usage();
            system.refresh_memory();
            networks.refresh(true);
            components.refresh(true);

            let sampled_at = Instant::now();
            let elapsed = sampled_at
                .duration_since(last_net_refresh)
                .as_secs_f64()
                .max(0.05);
            last_net_refresh = sampled_at;

            let mut rx_bytes = 0u64;
            let mut tx_bytes = 0u64;
            let mut rx_total = 0u64;
            let mut tx_total = 0u64;
            let mut busiest_iface = String::from("—");
            let mut busiest_total = 0u64;

            for (name, data) in networks.iter() {
                if is_loopback(name) {
                    continue;
                }

                rx_bytes = rx_bytes.saturating_add(data.received());
                tx_bytes = tx_bytes.saturating_add(data.transmitted());
                rx_total = rx_total.saturating_add(data.total_received());
                tx_total = tx_total.saturating_add(data.total_transmitted());

                let total = data
                    .total_received()
                    .saturating_add(data.total_transmitted());
                if total > busiest_total {
                    busiest_total = total;
                    busiest_iface = name.clone();
                }
            }

            let ram_total = system.total_memory();
            let ram_used = system.used_memory();
            let ram_pct = if ram_total > 0 {
                (ram_used as f64 / ram_total as f64 * 100.0) as f32
            } else {
                0.0
            };

            let ping_raw = ping_us.load(Ordering::Relaxed);
            let ping = if ping_raw == PING_NONE {
                None
            } else {
                Some(round1(ping_raw as f64 / 1_000.0) as f32)
            };

            let load = System::load_average();
            let metrics = Metrics {
                ts: now_ms(),
                cpu: system.global_cpu_usage().clamp(0.0, 100.0),
                cpu_cores: cores,
                ram_used,
                ram_total,
                ram_pct: ram_pct.clamp(0.0, 100.0),
                temp: hottest_component(&components),
                net_rx: round1(rx_bytes as f64 / elapsed),
                net_tx: round1(tx_bytes as f64 / elapsed),
                net_rx_total: rx_total,
                net_tx_total: tx_total,
                ping,
                uptime: System::uptime(),
                load: [
                    round2(load.one as f32),
                    round2(load.five as f32),
                    round2(load.fifteen as f32),
                ],
                host: host.clone(),
                os: os.clone(),
                iface: busiest_iface,
            };

            {
                let mut guard = latest.write().await;
                *guard = Some(metrics.clone());
            }

            {
                let mut points = history.lock().await;
                points.push_back(HistoryPoint {
                    t: metrics.ts,
                    rx: metrics.net_rx,
                    tx: metrics.net_tx,
                    cpu: metrics.cpu,
                    ram: metrics.ram_pct,
                });
                while points.len() > HISTORY_LEN {
                    points.pop_front();
                }
            }

            if clients.load(Ordering::Relaxed) > 0 {
                let envelope = MetricsEnvelope {
                    kind: "metrics",
                    metrics: &metrics,
                };
                if let Ok(payload) = serde_json::to_string(&envelope) {
                    // No receivers simply means every client left between the
                    // count check and the send.
                    let _ = tx.send(Arc::new(payload));
                }
            }
        }
    });
}

/// Measure TCP handshake latency to `target` (e.g. `1.1.1.1:443`).
///
/// A TCP connect is used instead of ICMP because it needs no privileges and
/// works identically on Linux, Windows and macOS.
pub fn spawn_ping(
    ping_us: Arc<AtomicU32>,
    clients: Arc<AtomicUsize>,
    activity: Arc<AtomicU64>,
    target: String,
) {
    tokio::spawn(async move {
        let mut first = true;
        loop {
            let active = clients.load(Ordering::Relaxed) > 0
                || now_secs().saturating_sub(activity.load(Ordering::Relaxed)) < 10;

            if active || first {
                let measured = measure_ping(&target).await;
                ping_us.store(measured, Ordering::Relaxed);
                first = false;
            }

            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

async fn measure_ping(target: &str) -> u32 {
    let started = Instant::now();
    let attempt = tokio::time::timeout(
        Duration::from_millis(1_200),
        tokio::net::TcpStream::connect(target),
    )
    .await;

    match attempt {
        Ok(Ok(stream)) => {
            drop(stream);
            started.elapsed().as_micros().min(u128::from(PING_NONE) - 1) as u32
        }
        _ => PING_NONE,
    }
}

fn hottest_component(components: &Components) -> Option<f32> {
    components
        .iter()
        .filter_map(|component| component.temperature())
        .filter(|value| value.is_finite() && *value > 0.0 && *value < 200.0)
        .fold(None::<f32>, |hottest, value| match hottest {
            Some(current) => Some(current.max(value)),
            None => Some(value),
        })
        .map(|value| round1(f64::from(value)) as f32)
}

fn is_loopback(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "lo" || lower.starts_with("lo0") || lower.contains("loopback")
}

fn round1(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn round2(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_interfaces_are_ignored() {
        assert!(is_loopback("lo"));
        assert!(is_loopback("lo0"));
        assert!(is_loopback("Loopback Pseudo-Interface 1"));
        assert!(!is_loopback("eth0"));
        assert!(!is_loopback("Wi-Fi"));
    }

    #[test]
    fn rounds_rates_to_one_decimal() {
        assert_eq!(round1(12.34), 12.3);
        assert_eq!(round1(12.36), 12.4);
    }
}
