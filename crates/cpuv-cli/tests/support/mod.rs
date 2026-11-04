use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

type Routes = Arc<Mutex<BTreeMap<String, Vec<u8>>>>;
pub struct Server {
    pub url: String,
    pub routes: Routes,
    pub requests: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    pub fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let routes: Routes = Default::default();
        let requests = Arc::new(AtomicUsize::new(0));
        let stop = Arc::new(AtomicBool::new(false));
        let (data, count, finished) = (routes.clone(), requests.clone(), stop.clone());
        let worker = thread::spawn(move || {
            while !finished.load(Ordering::SeqCst) {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(3)))
                        .unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0; 1024];
                    while request.len() < 8192 && !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        match stream.read(&mut buffer) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => request.extend_from_slice(&buffer[..n]),
                        }
                    }
                    let request = String::from_utf8_lossy(&request);
                    let Some(route) = request.split_whitespace().nth(1) else {
                        continue;
                    };
                    count.fetch_add(1, Ordering::SeqCst);
                    let body = data.lock().unwrap().get(route).cloned();
                    let status = if body.is_some() {
                        "200 OK"
                    } else {
                        "503 Service Unavailable"
                    };
                    let body = body.unwrap_or_default();
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    );
                    let _ = stream.write_all(&body);
                } else {
                    thread::sleep(Duration::from_millis(10));
                }
            }
        });
        Self {
            url,
            routes,
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
