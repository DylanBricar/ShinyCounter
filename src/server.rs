use anyhow::{anyhow, Result};
use parking_lot::{Condvar, Mutex};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tiny_http::{Header, Method, Response, Server};

// Long-polling approach chosen over SSE:
// tiny_http owns the TCP stream after `req.respond()` - there is no public
// API to keep writing to the same connection after that call returns.
// The `Response::from_reader` path finalises the response when the reader
// returns EOF, not when we choose to flush, so true SSE is not feasible
// without replacing the HTTP library.
//
// Long-polling (`GET /poll?since=<version>`) is simpler and has identical
// latency characteristics for this use-case: the server blocks on a Condvar
// until the version advances (or a 25 s timeout fires), then returns the new
// count as plain text.  The browser JS loops immediately on the returned
// value, giving sub-millisecond update latency in practice.

#[derive(Debug, Clone)]
pub struct OverlayState {
    pub count: u32,
    pub preset: String,
    pub armed: bool,
    pub styled: bool,
    /// Monotonically increasing; bumped on every `update()` call so
    /// long-poll handlers can detect real changes vs. spurious wakeups.
    pub version: u64,
}

impl Default for OverlayState {
    fn default() -> Self {
        Self {
            count: 0,
            preset: String::new(),
            armed: true,
            styled: true,
            version: 0,
        }
    }
}

pub struct CounterServer {
    /// Shared state + condvar pair.  The Condvar is notified on every update
    /// so `/poll` handlers wake up promptly.
    state: Arc<(Mutex<OverlayState>, Condvar)>,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    pub port: u16,
}

impl CounterServer {
    pub fn start(port: u16) -> Result<Self> {
        let addr = (IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let server = Server::http(addr).map_err(|e| anyhow!("http listen {port}: {e}"))?;
        let actual_port = server
            .server_addr()
            .to_ip()
            .map(|addr| addr.port())
            .unwrap_or(port);
        let state = Arc::new((Mutex::new(OverlayState::default()), Condvar::new()));
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
            port: actual_port,
        })
    }

    pub fn update(&self, count: u32, preset: String, armed: bool, styled: bool) {
        let (lock, cvar) = &*self.state;
        let mut s = lock.lock();
        s.count = count;
        s.preset = preset;
        s.armed = armed;
        s.styled = styled;
        s.version = s.version.wrapping_add(1);
        // Wake all blocked /poll handlers.
        cvar.notify_all();
    }
}

impl Drop for CounterServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        // Wake any blocked /poll threads so they notice the shutdown flag.
        let (_, cvar) = &*self.state;
        cvar.notify_all();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn run(server: Server, state: Arc<(Mutex<OverlayState>, Condvar)>, shutdown: Arc<AtomicBool>) {
    while !shutdown.load(Ordering::Relaxed) {
        match server.recv_timeout(Duration::from_millis(200)) {
            Ok(Some(req)) => handle_request(req, &state, &shutdown),
            Ok(None) => continue,
            Err(_) => break,
        }
    }
}

fn handle_request(
    req: tiny_http::Request,
    state: &Arc<(Mutex<OverlayState>, Condvar)>,
    shutdown: &Arc<AtomicBool>,
) {
    if !matches!(req.method(), Method::Get) {
        let _ = req.respond(Response::from_string("method not allowed").with_status_code(405));
        return;
    }
    let url = req.url().to_string();
    let path = url.split('?').next().unwrap_or("/");
    match path {
        "/count" | "/count.txt" => {
            let (lock, _) = &**state;
            let snapshot = lock.lock().clone();
            let body = format!("{}", snapshot.count);
            let resp = Response::from_string(body)
                .with_header(text_header())
                .with_header(cors_header())
                .with_header(no_cache_header());
            let _ = req.respond(resp);
        }
        "/poll" => {
            // Long-poll: parse `?since=<version>`, block until version advances
            // or 25 s elapses, then return the new count as plain text.
            let raw_since: u64 = url
                .split('?')
                .nth(1)
                .and_then(|qs| {
                    qs.split('&').find_map(|kv| {
                        let (k, v) = kv.split_once('=')?;
                        if k == "since" {
                            v.parse().ok()
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or(0);

            let (lock, cvar) = &**state;
            let snapshot = {
                let mut guard = lock.lock();
                // Clamp `since` to the current version. Garbage / future values
                // (e.g. a stale client cache after a server restart that reset
                // the version counter) would otherwise block for the full 25 s
                // even though the next update is ready.
                let since = raw_since.min(guard.version);
                // `while` not `if`: handles spurious wakeups from the OS as
                // well as the `notify_all` we send from `Drop` on shutdown.
                // Without this, a spurious wake returns the same version the
                // client already has and the JS spins re-polling immediately.
                let deadline = std::time::Instant::now() + Duration::from_secs(25);
                while guard.version <= since && !shutdown.load(Ordering::Relaxed) {
                    let now = std::time::Instant::now();
                    if now >= deadline {
                        break;
                    }
                    let _ = cvar.wait_for(&mut guard, deadline - now);
                }
                guard.clone()
            };

            // Return `<version>:<count>` so the JS side can extract both in
            // one fetch without a second request.
            let body = format!("{}:{}", snapshot.version, snapshot.count);
            let resp = Response::from_string(body)
                .with_header(text_header())
                .with_header(cors_header())
                .with_header(no_cache_header());
            let _ = req.respond(resp);
        }
        "/" | "/index.html" => {
            let (lock, _) = &**state;
            let styled = lock.lock().styled;
            let body = if styled {
                OVERLAY_HTML_STYLED
            } else {
                OVERLAY_HTML_PLAIN
            };
            let resp = Response::from_string(body)
                .with_header(html_header())
                .with_header(cors_header())
                .with_header(no_cache_header());
            let _ = req.respond(resp);
        }
        _ => {
            let _ = req.respond(Response::from_string("not found").with_status_code(404));
        }
    }
}

// Pure HTML + JS, zero CSS.  Uses long-polling via /poll for low-latency
// updates; falls back to setInterval+fetch('/count.txt') on error so existing
// OBS setups never break.
const OVERLAY_HTML_PLAIN: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<title>Shiny Counter</title>
</head>
<body><span id="count">0</span>
<script>
  (function () {
    const el = document.getElementById('count');
    let version = 0;

    // Long-poll loop: GET /poll?since=<version>, server blocks until the
    // version advances (or 25 s timeout), returns "<version>:<count>".
    async function poll() {
      try {
        const r = await fetch('/poll?since=' + version, { cache: 'no-store' });
        if (!r.ok) throw new Error('non-ok');
        const text = (await r.text()).trim();
        const sep = text.indexOf(':');
        if (sep !== -1) {
          version = parseInt(text.slice(0, sep), 10);
          el.textContent = text.slice(sep + 1);
        }
        poll(); // immediately start the next long-poll
      } catch (_) {
        // Network blip - fall back to 1 s polling via /count.txt
        fallback();
      }
    }

    function fallback() {
      setInterval(async function () {
        try {
          const r = await fetch('/count.txt', { cache: 'no-store' });
          if (r.ok) el.textContent = (await r.text()).trim();
        } catch (_) {}
      }, 1000);
    }

    poll();
  })();
</script>
</body>
</html>"#;

const OVERLAY_HTML_STYLED: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<title>Shiny Counter</title>
<style>
  html, body {
    margin: 0;
    padding: 0;
    background: transparent;
    color: #ffffff;
    font-family: "Segoe UI", "Helvetica Neue", Arial, sans-serif;
    font-feature-settings: "tnum" 1;
  }
  #count {
    display: inline-block;
    padding: 12px 20px;
    font-size: 96px;
    font-weight: 700;
    line-height: 1;
    text-shadow: 0 0 6px rgba(0,0,0,0.55);
    font-variant-numeric: tabular-nums;
  }
</style>
</head>
<body>
<span id="count">0</span>
<script>
  // Long-poll loop: GET /poll?since=<version>.  Server blocks until the
  // count changes (up to 25 s), then returns "<version>:<count>".
  // Falls back to setInterval(fetch('/count.txt'), 1000) on any error so
  // existing OBS setups with custom CSS overlays never break.
  (function () {
    const el = document.getElementById('count');
    let version = 0;

    async function poll() {
      try {
        const r = await fetch('/poll?since=' + version, { cache: 'no-store' });
        if (!r.ok) throw new Error('non-ok');
        const text = (await r.text()).trim();
        const sep = text.indexOf(':');
        if (sep !== -1) {
          version = parseInt(text.slice(0, sep), 10);
          el.textContent = text.slice(sep + 1);
        }
        poll();
      } catch (_) {
        fallback();
      }
    }

    function fallback() {
      setInterval(async function () {
        try {
          const r = await fetch('/count.txt', { cache: 'no-store' });
          if (r.ok) el.textContent = (await r.text()).trim();
        } catch (_) {}
      }, 1000);
    }

    poll();
  })();
</script>
</body>
</html>"#;

fn text_header() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=utf-8"[..]).unwrap()
}

fn html_header() -> Header {
    Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap()
}

fn cors_header() -> Header {
    Header::from_bytes(&b"Access-Control-Allow-Origin"[..], &b"*"[..]).unwrap()
}

fn no_cache_header() -> Header {
    Header::from_bytes(
        &b"Cache-Control"[..],
        &b"no-store, no-cache, must-revalidate"[..],
    )
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_starts_on_random_port_and_serves_count() {
        let srv = CounterServer::start(0).expect("server should bind an ephemeral port");
        let port = srv.port;
        srv.update(42, "Test Preset".to_string(), true, true);
        std::thread::sleep(Duration::from_millis(50));

        let body = ureq_get(&format!("http://127.0.0.1:{port}/count")).expect("/count response");
        let last_line = body.lines().last().unwrap_or("").trim();
        assert_eq!(last_line, "42", "expected plain int body, got: {body:?}");

        // Verify the overlay HTML endpoint exists and references /poll.
        let body = ureq_get(&format!("http://127.0.0.1:{port}/")).expect("/ response");
        assert!(
            body.contains("/poll"),
            "overlay HTML should use /poll long-polling, got: {body:?}"
        );
    }

    #[test]
    fn long_poll_endpoint_returns_initial_then_updated_count() {
        let srv = CounterServer::start(0).expect("server should bind an ephemeral port");
        let port = srv.port;

        // Seed an initial value.
        srv.update(7, "Mon".to_string(), true, false);
        std::thread::sleep(Duration::from_millis(50));

        // /poll?since=0 should return immediately because version > 0.
        let body = ureq_get(&format!("http://127.0.0.1:{port}/poll?since=0"))
            .expect("initial /poll response");
        let last = body.lines().last().unwrap_or("").trim().to_string();
        let (version, count) = parse_poll_body(&last);
        assert_eq!(count, 7, "poll should return count 7, got: {last:?}");

        // Update to a new count; spawn a poll that starts before the update.
        let port2 = port;
        let poll_handle = std::thread::spawn(move || {
            // Request with since=current version so it will block briefly.
            ureq_get(&format!("http://127.0.0.1:{port2}/poll?since={version}"))
        });

        std::thread::sleep(Duration::from_millis(30));
        srv.update(99, "Mon".to_string(), true, false);

        let result = poll_handle
            .join()
            .expect("poll thread should not panic")
            .expect("updated /poll response");
        let last = result.lines().last().unwrap_or("").trim().to_string();
        let (_, count) = parse_poll_body(&last);
        assert_eq!(count, 99, "poll should return updated count, got: {last:?}");
    }

    fn parse_poll_body(body: &str) -> (u64, u32) {
        let (version, count) = body
            .split_once(':')
            .unwrap_or_else(|| panic!("poll response should be version:count, got: {body:?}"));
        (
            version.parse().expect("poll version should be a u64"),
            count.parse().expect("poll count should be a u32"),
        )
    }

    fn ureq_get(url: &str) -> Option<String> {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        let (_scheme, rest) = url.split_once("://")?;
        let (hostport, path_and_query) = rest
            .split_once('/')
            .map(|(a, b)| (a, format!("/{b}")))
            .unwrap_or((rest, "/".to_string()));
        let mut stream = TcpStream::connect(hostport).ok()?;
        stream
            .set_read_timeout(Some(Duration::from_millis(500)))
            .ok()?;
        write!(
            stream,
            "GET {path_and_query} HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\n\r\n"
        )
        .ok()?;
        let mut buf = String::new();
        stream.read_to_string(&mut buf).ok()?;
        Some(buf)
    }
}
