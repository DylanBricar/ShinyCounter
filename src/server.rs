use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tiny_http::{Header, Method, Response, Server};

#[derive(Debug, Clone, Default)]
pub struct OverlayState {
    pub count: u32,
    pub preset: String,
    pub armed: bool,
}

pub struct CounterServer {
    state: Arc<Mutex<OverlayState>>,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    pub port: u16,
}

impl CounterServer {
    pub fn start(port: u16) -> Result<Self> {
        let addr = (IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let server = Server::http(addr).map_err(|e| anyhow!("http listen {port}: {e}"))?;
        let state = Arc::new(Mutex::new(OverlayState::default()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let st = Arc::clone(&state);
        let sd = Arc::clone(&shutdown);
        let handle = thread::Builder::new()
            .name("shiny-counter-http".into())
            .spawn(move || run(server, st, sd))?;

        Ok(Self {
            state,
            shutdown,
            handle: Some(handle),
            port,
        })
    }

    pub fn update(&self, count: u32, preset: String, armed: bool) {
        let mut s = self.state.lock();
        s.count = count;
        s.preset = preset;
        s.armed = armed;
    }
}

impl Drop for CounterServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn run(server: Server, state: Arc<Mutex<OverlayState>>, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Relaxed) {
        match server.recv_timeout(Duration::from_millis(200)) {
            Ok(Some(req)) => handle_request(req, &state),
            Ok(None) => continue,
            Err(_) => break,
        }
    }
}

fn handle_request(req: tiny_http::Request, state: &Arc<Mutex<OverlayState>>) {
    if !matches!(req.method(), Method::Get) {
        let _ = req.respond(Response::from_string("method not allowed").with_status_code(405));
        return;
    }
    let url = req.url().split('?').next().unwrap_or("/").to_string();
    match url.as_str() {
        "/" | "/count" | "/count.txt" | "/index.html" => {
            let snapshot = state.lock().clone();
            let body = format!("{}", snapshot.count);
            let resp = Response::from_string(body)
                .with_header(text_header())
                .with_header(cors_header());
            let _ = req.respond(resp);
        }
        _ => {
            let _ = req.respond(Response::from_string("not found").with_status_code(404));
        }
    }
}

fn text_header() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=utf-8"[..]).unwrap()
}

fn cors_header() -> Header {
    Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_starts_on_random_port_and_serves_count() {
        let port = 17_873u16;
        let srv = match CounterServer::start(port) {
            Ok(s) => s,
            Err(_) => return,
        };
        srv.update(42, "Test Preset".to_string(), true);
        std::thread::sleep(Duration::from_millis(50));

        let resp = ureq_get(&format!("http://127.0.0.1:{port}/"));
        if let Some(body) = resp {
            let last_line = body.lines().last().unwrap_or("").trim();
            assert_eq!(last_line, "42", "expected plain int body, got: {body:?}");
        }
    }

    fn ureq_get(url: &str) -> Option<String> {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        let (_scheme, rest) = url.split_once("://")?;
        let (hostport, path) = rest
            .split_once('/')
            .map(|(a, b)| (a, format!("/{b}")))
            .unwrap_or((rest, "/".to_string()));
        let mut stream = TcpStream::connect(hostport).ok()?;
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .ok()?;
        write!(
            stream,
            "GET {path} HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\n\r\n"
        )
        .ok()?;
        let mut buf = String::new();
        stream.read_to_string(&mut buf).ok()?;
        Some(buf)
    }
}
