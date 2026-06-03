//! Host-mode connection collector. Each platform observes local connections
//! through its native facility and exposes the same pull-based API:
//! `NetworkCollector::new()` + `collect_connections() -> Vec<NetworkEvent>`.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::NetworkCollector;

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::NetworkCollector;

#[cfg(not(any(target_os = "linux", windows)))]
mod fallback;
#[cfg(not(any(target_os = "linux", windows)))]
pub use fallback::NetworkCollector;
