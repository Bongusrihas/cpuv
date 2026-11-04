use super::*;
use std::{
    fs,
    io::Write,
    net::{TcpListener, TcpStream},
    sync::atomic::AtomicBool,
    thread,
};

struct Server {
    url: String,
    requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(responses: Vec<Vec<u8>>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (count, done) = (requests.clone(), stop.clone());
        let worker = thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(2)))
                        .unwrap();
                    let mut header = Vec::new();
                    let mut chunk = [0; 1024];
                    while header.len() < 8192 && !header.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut chunk) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => header.extend_from_slice(&chunk[..n]),
                        }
                    }
                    if !header.windows(4).any(|w| w == b"\r\n\r\n") {
                        continue;
                    }
                    let n = count.fetch_add(1, Ordering::SeqCst);
                    let _ = stream.write_all(&responses[n.min(responses.len() - 1)]);
                } else {
                    thread::sleep(Duration::from_millis(5));
                }
            }
        });
        Self {
            url,
            requests,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.url.trim_start_matches("http://"));
        self.worker.take().unwrap().join().unwrap();
    }
}
fn response(status: &str, body: &[u8]) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}
fn package(bytes: &[u8]) -> Package {
    Package {
        name: format!("{}fixture", cpuv_solver::PREFIX),
        base: None,
        version: "1-1".into(),
        filename: "fixture.pkg.tar.zst".into(),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        size: bytes.len() as u64,
        depends: vec![],
        provides: vec![],
        conflicts: vec![],
    }
}
fn database(duplicate: bool) -> Vec<u8> {
    let p = package(b"package");
    let desc = format!(
        "%NAME%\n{}\n\n%VERSION%\n{}\n\n%FILENAME%\n{}\n\n%SHA256SUM%\n{}\n\n%CSIZE%\n{}\n",
        p.name, p.version, p.filename, p.sha256, p.size
    );
    let mut tar = tar::Builder::new(Vec::new());
    for name in if duplicate {
        vec!["first/desc", "second/desc"]
    } else {
        vec!["first/desc"]
    } {
        let mut header = tar::Header::new_gnu();
        header.set_size(desc.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        tar.append_data(&mut header, name, desc.as_bytes()).unwrap();
    }
    zstd::stream::encode_all(&tar.into_inner().unwrap()[..], 1).unwrap()
}
#[test]
fn index_parser_rejects_duplicates_wrong_namespaces_limits_and_truncation() {
    let data = database(false);
    assert_eq!(parse_database(&data).unwrap().len(), 1);
    assert!(parse_database(&database(true)).is_err());
    assert!(parse_database_for(&data, "wrong-prefix-").is_err());
    assert!(parse_database(&data[..data.len() / 2]).is_err());
    assert!(
        parse_database_with_limits(
            &data,
            cpuv_solver::PREFIX,
            &Limits {
                index_bytes: 8,
                ..Limits::default()
            }
        )
        .is_err()
    );
}
#[tokio::test]
async fn index_retries_repairs_corruption_and_preserves_offline_failure() {
    let server = Server::new(vec![
        response("503 Service Unavailable", b""),
        response("200 OK", &database(false)),
    ]);
    let root = tempfile::tempdir().unwrap();
    let fetcher = Fetcher::new(server.url.clone(), root.path().into(), 2, false).unwrap();
    assert_eq!(fetcher.index(false).await.unwrap().len(), 1);
    let online_requests = server.requests.load(Ordering::SeqCst);
    assert!((2..=3).contains(&online_requests));
    let index = fs::read_dir(root.path().join("index"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "db"))
        .unwrap();
    fs::write(&index, b"damaged").unwrap();
    let offline = Fetcher::new(server.url.clone(), root.path().into(), 2, true).unwrap();
    assert!(offline.index(false).await.is_err());
    assert_eq!(server.requests.load(Ordering::SeqCst), online_requests);
    fetcher.index(false).await.unwrap();
    assert!((1..=3).contains(&(server.requests.load(Ordering::SeqCst) - online_requests)));
    assert_eq!(
        fs::read_dir(root.path().join("index/quarantine"))
            .unwrap()
            .count(),
        1
    );
}
#[tokio::test]
async fn downloads_retry_verify_sizes_hashes_and_support_offline_reuse() {
    let payload = b"verified archive";
    let server = Server::new(vec![
        response("503 Service Unavailable", b""),
        response("200 OK", payload),
    ]);
    let root = tempfile::tempdir().unwrap();
    let p = package(payload);
    let fetcher = Fetcher::new(server.url.clone(), root.path().into(), 2, false).unwrap();
    let (_, stats) = fetcher
        .download_all(std::slice::from_ref(&p))
        .await
        .unwrap();
    assert_eq!(stats.downloaded, 1);
    let online_requests = server.requests.load(Ordering::SeqCst);
    // The deliberate 503 must retry; a Windows loopback abort can consume another attempt.
    assert!((2..=3).contains(&online_requests));
    let offline = Fetcher::new(server.url.clone(), root.path().into(), 2, true).unwrap();
    let (_, stats) = offline
        .download_all(std::slice::from_ref(&p))
        .await
        .unwrap();
    assert_eq!(stats.cached_archives, 1);
    assert_eq!(server.requests.load(Ordering::SeqCst), online_requests);
    fs::write(
        root.path()
            .join("archives")
            .join(format!("{}.pkg.tar.zst", p.sha256)),
        b"changed",
    )
    .unwrap();
    assert!(
        offline
            .download_all(std::slice::from_ref(&p))
            .await
            .is_err()
    );
    fetcher
        .download_all(std::slice::from_ref(&p))
        .await
        .unwrap();
    for body in [
        b"different bytes!".as_slice(),
        b"too long archive beyond size".as_slice(),
    ] {
        let bad = Server::new(vec![response("200 OK", body)]);
        let cache = tempfile::tempdir().unwrap();
        assert!(
            Fetcher::new(bad.url.clone(), cache.path().into(), 1, false)
                .unwrap()
                .download_all(std::slice::from_ref(&p))
                .await
                .is_err()
        );
        assert!(
            !cache
                .path()
                .join("archives")
                .join(format!("{}.pkg.tar.zst", p.sha256))
                .exists()
        );
    }
}
#[test]
fn only_official_repository_namespaces_select_an_archive_mirror() {
    assert_eq!(
        official_archive_mirror(DEFAULT_REPOSITORY).as_deref(),
        Some("https://mirror.msys2.org/mingw/ucrt64")
    );
    for url in [
        "https://repo.msys2.org/mingw/custom",
        "https://mirror.msys2.org/mingw/ucrt64",
        "http://127.0.0.1/mingw/ucrt64",
    ] {
        assert!(official_archive_mirror(url).is_none());
    }
}

#[tokio::test]
async fn archive_retry_uses_mirror_without_relaxing_the_index_digest() {
    let primary = Server::new(vec![response("503 Service Unavailable", b"")]);
    let payload = b"archive";
    let mirror = Server::new(vec![response("200 OK", payload)]);
    let root = tempfile::tempdir().unwrap();
    let mut fetcher = Fetcher::new(primary.url.clone(), root.path().into(), 1, false).unwrap();
    fetcher.archive_mirror = Some(mirror.url.clone());
    let p = package(payload);
    let (archives, _) = fetcher
        .download_all(std::slice::from_ref(&p))
        .await
        .unwrap();
    assert_eq!(fs::read(&archives[0].1).unwrap(), payload);
    assert_eq!(primary.requests.load(Ordering::SeqCst), 1);
    assert_eq!(mirror.requests.load(Ordering::SeqCst), 1);
    fs::remove_file(&archives[0].1).unwrap();

    let corrupt = Server::new(vec![response("200 OK", b"changed")]);
    fetcher.archive_mirror = Some(corrupt.url.clone());
    assert!(fetcher.download_all(&[p]).await.is_err());
    assert!(!archives[0].1.exists());
    assert_eq!(corrupt.requests.load(Ordering::SeqCst), 1);
}

#[test]
fn download_policy_rejects_remote_plain_http() {
    for url in ["http://example.com/a", "ftp://localhost/a"] {
        assert!(!policy::allowed_url(&reqwest::Url::parse(url).unwrap()));
    }
    for url in [
        "https://example.com/a",
        "http://127.0.0.1:42/a",
        "http://[::1]/a",
    ] {
        assert!(policy::allowed_url(&reqwest::Url::parse(url).unwrap()));
    }
}
