//! Shared transfer limits and HTTPS/loopback policy for archive clients.
use anyhow::{Result, ensure};
use std::{net::IpAddr, time::Duration};

#[derive(Clone, Debug)]
pub struct Limits {
    pub index_bytes: u64,
    pub archive_bytes: u64,
    pub expanded_bytes: u64,
    pub entries: u64,
    /// Zero disables the whole-transfer deadline; connection/read timeouts remain.
    pub timeout_secs: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            index_bytes: 64 * 1024 * 1024,
            archive_bytes: 16 * 1024 * 1024 * 1024,
            expanded_bytes: 64 * 1024 * 1024 * 1024,
            entries: 1_000_000,
            timeout_secs: 3600,
        }
    }
}

pub fn allowed_url(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        || url.scheme() == "http"
            && url.host_str().is_some_and(|host| {
                host.eq_ignore_ascii_case("localhost")
                    || host
                        .trim_matches(['[', ']'])
                        .parse::<IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())
            })
}

pub fn client(concurrency: usize, limits: &Limits) -> Result<reqwest::Client> {
    ensure!(
        limits.index_bytes > 0
            && limits.archive_bytes > 0
            && limits.expanded_bytes > 0
            && limits.entries > 0,
        "Download and extraction limits must be positive"
    );
    let mut builder = reqwest::Client::builder()
        .user_agent(concat!("cpuv/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .pool_max_idle_per_host(concurrency)
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 || !allowed_url(attempt.url()) {
                attempt.error("Unsafe archive redirect")
            } else {
                attempt.follow()
            }
        }));
    if limits.timeout_secs != 0 {
        builder = builder.timeout(Duration::from_secs(limits.timeout_secs));
    }
    Ok(builder.build()?)
}
