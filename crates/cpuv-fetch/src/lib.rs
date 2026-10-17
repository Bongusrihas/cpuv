use anyhow::{Context, Result, bail, ensure};
use cpuv_solver::{Index, Package, parse_desc};
use futures_util::{StreamExt, TryStreamExt, stream};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const DEFAULT_REPOSITORY: &str = "https://repo.msys2.org/mingw/ucrt64";

pub mod policy;
pub use policy::Limits;

#[derive(Clone)]
pub struct Fetcher {
    client: reqwest::Client,
    pub repository: String,
    pub cache: PathBuf,
    pub concurrency: usize,
    pub offline: bool,
    pub package_prefix: String,
    pub database: String,
    pub limits: Limits,
}

#[derive(Debug, Default, Serialize)]
pub struct DownloadStats {
    pub downloaded: usize,
    pub cached_archives: usize,
    pub bytes: u64,
    pub peak_concurrent_downloads: usize,
    pub elapsed_ms: u128,
}

impl Fetcher {
    pub fn new(
        repository: String,
        cache: PathBuf,
        concurrency: usize,
        offline: bool,
    ) -> Result<Self> {
        ensure!(
            (1..=256).contains(&concurrency),
            "Download concurrency must be between 1 and 256"
        );
        Self::with_limits(repository, cache, concurrency, offline, Limits::default())
    }

    pub fn with_limits(
        repository: String,
        cache: PathBuf,
        concurrency: usize,
        offline: bool,
        limits: Limits,
    ) -> Result<Self> {
        ensure!(
            (1..=256).contains(&concurrency),
            "Download concurrency must be between 1 and 256"
        );
        let url = reqwest::Url::parse(&repository)?;
        ensure!(
            policy::allowed_url(&url),
            "Use HTTPS for repositories; HTTP is allowed only for local loopback servers"
        );
        let client = policy::client(concurrency, &limits)?;
        Ok(Self {
            client,
            repository: repository.trim_end_matches('/').into(),
            cache,
            concurrency,
            offline,
            package_prefix: cpuv_solver::PREFIX.into(),
            database: "ucrt64.db".into(),
            limits,
        })
    }

    pub fn with_namespace(mut self, prefix: &str, database: &str) -> Self {
        self.package_prefix = prefix.into();
        self.database = database.into();
        self
    }

    pub async fn index(&self, refresh: bool) -> Result<Index> {
        let key = format!("{:x}", Sha256::digest(self.repository.as_bytes()));
        let dir = self.cache.join("index");
        tokio::fs::create_dir_all(&dir).await?;
        let file = dir.join(format!("{key}.db"));
        let fresh = std::fs::metadata(&file)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|d| d < Duration::from_secs(3600));
        if self.offline || (!refresh && fresh) {
            ensure!(
                tokio::fs::metadata(&file)
                    .await
                    .context("No cached repository index; run once online")?
                    .len()
                    <= self.limits.index_bytes,
                "Repository index exceeds --max-index-mib (this is metadata, not a package archive)"
            );
            let bytes = tokio::fs::read(&file)
                .await
                .context("No cached repository index; run once online")?;
            let prefix = self.package_prefix.clone();
            let limits = self.limits.clone();
            return tokio::task::spawn_blocking(move || {
                parse_database_with_limits(&bytes, &prefix, &limits)
            })
            .await?;
        }
        let url = format!("{}/{}", self.repository, self.database);
        let mut last = None;
        for attempt in 0..3 {
            let result: Result<(Vec<u8>, Index)> = async {
                let response = self.client.get(&url).send().await?.error_for_status()?;
                ensure!(response.content_length().is_none_or(|size| size <= self.limits.index_bytes),
                    "Repository index exceeds --max-index-mib (metadata limit, not package size)");
                let mut chunks = response.bytes_stream();
                let mut bytes = Vec::new();
                while let Some(chunk) = chunks.next().await {
                    let chunk = chunk?;
                    ensure!(
                        (bytes.len() as u64).saturating_add(chunk.len() as u64) <= self.limits.index_bytes,
                        "Repository index exceeds --max-index-mib (metadata limit, not package size)"
                    );
                    bytes.extend_from_slice(&chunk);
                }
                let copy = bytes.clone();
                let prefix = self.package_prefix.clone();
                let limits = self.limits.clone();
                let index = tokio::task::spawn_blocking(move || parse_database_with_limits(&copy, &prefix, &limits))
                    .await??;
                Ok((bytes, index))
            }
            .await;
            match result {
                Ok((bytes, index)) => {
                    let temp = tempfile::NamedTempFile::new_in(&dir)?;
                    tokio::fs::write(temp.path(), &bytes).await?;
                    temp.persist(&file).map_err(|e| e.error)?;
                    return Ok(index);
                }
                Err(e) => {
                    last = Some(e);
                    tokio::time::sleep(Duration::from_millis(200 << attempt)).await;
                }
            }
        }
        Err(last.unwrap()).context(format!("Fetching repository {url}"))
    }

    pub async fn download_all(
        &self,
        packages: &[Package],
    ) -> Result<(Vec<(Package, PathBuf)>, DownloadStats)> {
        let start = Instant::now();
        let dir = self.cache.join("archives");
        tokio::fs::create_dir_all(&dir).await?;
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let progress = MultiProgress::new();
        let tasks = stream::iter(packages.iter().cloned().map(|p| {
            let active = active.clone();
            let peak = peak.clone();
            let dir = dir.clone();
            let progress = &progress;
            async move {
                ensure!(
                    p.size <= self.limits.archive_bytes,
                    "Package {} exceeds --max-download-mib",
                    p.name
                );
                let path = dir.join(format!("{}.pkg.tar.zst", p.sha256));
                if path.exists() {
                    if verify_file(&path, &p.sha256, p.size).await.is_ok() {
                        return Ok((p, path, true, 0));
                    }
                    tokio::fs::remove_file(&path).await?;
                }
                ensure!(
                    !self.offline,
                    "Missing cached archive for {} {}; offline sync cannot download",
                    p.name,
                    p.version
                );
                let n = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(n, Ordering::SeqCst);
                struct Guard(Arc<AtomicUsize>);
                impl Drop for Guard {
                    fn drop(&mut self) {
                        self.0.fetch_sub(1, Ordering::SeqCst);
                    }
                }
                let _guard = Guard(active);
                let bar = progress.add(ProgressBar::new(p.size));
                bar.set_style(ProgressStyle::with_template(
                    "{msg:30} {bar:20.cyan/blue} {bytes}/{total_bytes} {bytes_per_sec}",
                )?);
                bar.set_message(p.name.trim_start_matches(cpuv_solver::PREFIX).to_owned());
                let mut last = None;
                for attempt in 0..3 {
                    bar.set_position(0);
                    match self.download_one(&p, &path, &bar).await {
                        Ok(bytes) => {
                            bar.finish_and_clear();
                            return Ok((p, path, false, bytes));
                        }
                        Err(e) => {
                            last = Some(e);
                            tokio::time::sleep(Duration::from_millis(200 << attempt)).await;
                        }
                    }
                }
                bar.abandon_with_message("failed");
                Err(last.unwrap()).with_context(|| format!("Downloading {}", p.filename))
            }
        }))
        .buffer_unordered(self.concurrency);
        let results: Vec<(Package, PathBuf, bool, u64)> = tasks.try_collect().await?;
        let mut stats = DownloadStats {
            peak_concurrent_downloads: peak.load(Ordering::SeqCst),
            elapsed_ms: start.elapsed().as_millis(),
            ..Default::default()
        };
        let paths = results
            .into_iter()
            .map(|(p, path, cached, bytes)| {
                if cached {
                    stats.cached_archives += 1;
                } else {
                    stats.downloaded += 1;
                    stats.bytes += bytes;
                }
                (p, path)
            })
            .collect();
        Ok((paths, stats))
    }

    async fn download_one(&self, p: &Package, path: &Path, bar: &ProgressBar) -> Result<u64> {
        ensure!(
            p.size <= self.limits.archive_bytes,
            "Package exceeds --max-download-mib"
        );
        let temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
        let mut file = tokio::fs::File::create(temp.path()).await?;
        let mut hasher = Sha256::new();
        let mut bytes = 0;
        let response = self
            .client
            .get(format!("{}/{}", self.repository, p.filename))
            .send()
            .await?
            .error_for_status()?;
        let mut chunks = response.bytes_stream();
        while let Some(chunk) = chunks.next().await {
            let chunk = chunk?;
            bytes += chunk.len() as u64;
            ensure!(bytes <= p.size, "{} exceeds its indexed size", p.filename);
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            bar.set_position(bytes);
        }
        file.flush().await?;
        drop(file);
        ensure!(
            bytes == p.size,
            "Truncated archive {}: expected {}, received {bytes}",
            p.filename,
            p.size
        );
        ensure!(
            format!("{:x}", hasher.finalize()) == p.sha256,
            "SHA-256 mismatch for {}; refusing extraction",
            p.filename
        );
        temp.persist(path).map_err(|e| e.error)?;
        Ok(bytes)
    }
}

pub async fn verify_file(path: &Path, sha256: &str, expected_size: u64) -> Result<()> {
    let mut file = tokio::fs::File::open(path).await?;
    ensure!(
        file.metadata().await?.len() == expected_size,
        "Cached archive size mismatch"
    );
    let mut hasher = Sha256::new();
    let mut buf = vec![0; 128 * 1024];
    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    ensure!(
        format!("{:x}", hasher.finalize()) == sha256,
        "Cached archive checksum mismatch"
    );
    Ok(())
}

pub fn parse_database(bytes: &[u8]) -> Result<Index> {
    parse_database_for(bytes, cpuv_solver::PREFIX)
}

pub fn parse_database_for(bytes: &[u8], prefix: &str) -> Result<Index> {
    parse_database_with_limits(bytes, prefix, &Limits::default())
}

pub fn parse_database_with_limits(bytes: &[u8], prefix: &str, limits: &Limits) -> Result<Index> {
    ensure!(
        bytes.len() as u64 <= limits.index_bytes,
        "Repository index exceeds --max-index-mib"
    );
    let reader: Box<dyn Read> = if bytes.starts_with(&[0x28, 0xb5, 0x2f, 0xfd]) {
        Box::new(zstd::stream::read::Decoder::new(Cursor::new(bytes))?)
    } else if bytes.starts_with(&[0x1f, 0x8b]) {
        Box::new(flate2::read::GzDecoder::new(Cursor::new(bytes)))
    } else {
        bail!("Unsupported repository database compression (expected zstd or gzip)");
    };
    let mut archive = tar::Archive::new(reader.take(limits.index_bytes.saturating_mul(16)));
    let mut index = Index::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.file_name().is_some_and(|s| s == "desc") {
            ensure!(entry.size() <= 1024 * 1024, "Oversized repository entry");
            let mut desc = String::new();
            entry.read_to_string(&mut desc)?;
            let package = parse_desc(&desc)?;
            ensure!(
                package.name.starts_with(prefix),
                "Repository package {} does not match the selected namespace {prefix}",
                package.name
            );
            ensure!(
                index.insert(package.name.clone(), package).is_none(),
                "Duplicate package in repository snapshot"
            );
        }
    }
    ensure!(
        !index.is_empty(),
        "Repository database contains no packages"
    );
    Ok(index)
}
