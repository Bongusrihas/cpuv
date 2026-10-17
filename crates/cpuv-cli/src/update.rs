//! Download, verify, and replace the invoked executable without a shell or Rust.
use anyhow::{Context, Result, ensure};
use clap::Args;
use fs2::FileExt;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::{
    cmp::Ordering,
    fs::{self, OpenOptions},
    process::Command,
    time::Duration,
};

#[derive(Args, Debug)]
pub struct Options {
    /// Install a specific release instead of the latest stable release.
    #[arg(long, value_name = "VERSION")]
    pub version: Option<String>,
    /// Report availability without downloading or changing the executable.
    #[arg(long)]
    pub check: bool,
    /// Reinstall the same version, or explicitly allow a downgrade.
    #[arg(long)]
    pub force: bool,
    /// GitHub releases base URL; HTTPS required, except loopback test servers.
    #[arg(
        long,
        env = "CPUV_UPDATE_BASE_URL",
        default_value = "https://github.com/Bongusrihas/cpuv/releases"
    )]
    pub release_base_url: String,
}

fn version(text: &str) -> Result<&str> {
    let text = text.strip_prefix('v').unwrap_or(text);
    ensure!(
        text.split('.').count() == 3
            && text
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|c| c.is_ascii_digit())),
        "Invalid release version {text}; expected a stable version such as 0.0.1"
    );
    Ok(text)
}

fn checksum(text: &str) -> Result<String> {
    let fields: Vec<_> = text.split_whitespace().collect();
    ensure!(
        fields.len() == 2
            && fields[0].len() == 64
            && fields[0].bytes().all(|c| c.is_ascii_hexdigit())
            && fields[1].trim_start_matches('*') == "cpuv-setup.exe",
        "Invalid release checksum file"
    );
    Ok(fields[0].to_ascii_lowercase())
}

async fn download(client: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>> {
    let mut last = None;
    for attempt in 0..3 {
        let result: Result<Vec<u8>> = async {
            let response = client.get(url).send().await?.error_for_status()?;
            ensure!(
                response
                    .content_length()
                    .is_none_or(|size| size <= limit as u64),
                "Release asset exceeds download limit"
            );
            let mut stream = response.bytes_stream();
            let mut data = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk?;
                ensure!(
                    data.len() + chunk.len() <= limit,
                    "Release asset exceeds download limit"
                );
                data.extend_from_slice(&chunk);
            }
            Ok(data)
        }
        .await;
        match result {
            Ok(bytes) => return Ok(bytes),
            Err(error) => last = Some(error),
        }
        if attempt < 2 {
            tokio::time::sleep(Duration::from_millis(200 << attempt)).await;
        }
    }
    Err(last.unwrap()).with_context(|| {
        format!("Downloading {url}; the release must contain published prebuilt assets")
    })
}

pub async fn run(options: &Options, offline: bool) -> Result<i32> {
    ensure!(
        !offline,
        "Self update requires internet access; omit --offline"
    );
    ensure!(
        cfg!(target_os = "windows") && cfg!(target_arch = "x86_64"),
        "Prebuilt self updates currently support Windows x64"
    );
    let base = reqwest::Url::parse(&options.release_base_url)?;
    ensure!(
        cpuv_fetch::policy::allowed_url(&base),
        "Release URL must use HTTPS (loopback HTTP is allowed for tests)"
    );
    ensure!(
        base.query().is_none() && base.fragment().is_none(),
        "Release base URL must not contain a query or fragment"
    );
    let base = options.release_base_url.trim_end_matches('/');
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error("Too many release redirects");
            }
            let url = attempt.url();
            if cpuv_fetch::policy::allowed_url(url) {
                attempt.follow()
            } else {
                attempt.error("Refusing an insecure release redirect")
            }
        }))
        .user_agent(concat!("cpuv/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .build()?;
    let target = if let Some(requested) = &options.version {
        version(requested)?.to_owned()
    } else {
        let response = client.get(format!("{base}/latest")).send().await?
            .error_for_status().context("No published cpuv release found; publish the GitHub Release and prebuilt assets first")?;
        let location = response.url();
        let marker = format!(
            "{}/tag/",
            reqwest::Url::parse(base)?.path().trim_end_matches('/')
        );
        let tag = location
            .path()
            .strip_prefix(&marker)
            .context("Latest release did not redirect to a versioned release tag")?;
        version(tag)?.to_owned()
    };
    let current = env!("CARGO_PKG_VERSION");
    let ordering = cpuv_solver::version_cmp(&target, current);
    if options.check {
        if ordering == Ordering::Greater {
            println!("Update available: cpuv {current} -> {target}. Run cpuv self update.");
        } else {
            println!("cpuv {current} is up to date (release {target}).");
        }
        return Ok(0);
    }
    if ordering == Ordering::Equal && !options.force {
        println!("cpuv {current} is already up to date.");
        return Ok(0);
    }
    ensure!(
        ordering != Ordering::Less || options.force,
        "Refusing downgrade from {current} to {target}; use --force to request it explicitly"
    );
    let executable = std::env::current_exe()?;
    let directory = executable.parent().context("Executable has no directory")?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join(".cpuv-self-update.lock"))?;
    lock.try_lock_exclusive()
        .context("Another self update is in progress")?;
    let asset = format!("{base}/download/v{target}/cpuv-setup.exe");
    eprintln!("Downloading cpuv {target}...");
    let declared = download(&client, &format!("{asset}.sha256"), 4096).await?;
    let expected = checksum(std::str::from_utf8(&declared)?)?;
    let bytes = download(&client, &asset, 128 * 1024 * 1024).await?;
    ensure!(
        format!("{:x}", Sha256::digest(&bytes)) == expected,
        "Release checksum mismatch; the installed executable was not changed"
    );
    ensure!(
        bytes.starts_with(b"MZ"),
        "Release asset is not a Windows executable"
    );
    let temporary = tempfile::Builder::new().prefix("cpuv-update-").tempdir()?;
    // Do not name it cpuv-setup.exe: version validation must dispatch as the CLI.
    let candidate = temporary.path().join("cpuv-candidate.exe");
    fs::write(&candidate, bytes)?;
    let output = Command::new(&candidate)
        .arg("--version")
        .output()
        .context("Checking downloaded cpuv executable")?;
    ensure!(
        output.status.success()
            && String::from_utf8_lossy(&output.stdout).trim() == format!("cpuv {target}"),
        "Downloaded executable version differs from release {target}; installation was not changed"
    );
    self_replace::self_replace(&candidate)
        .context("Replacing cpuv; close other running cpuv programs and retry")?;
    println!("Updated cpuv {current} -> {target}. Next invocation uses the new executable.");
    Ok(0)
}
