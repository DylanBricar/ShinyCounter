use anyhow::{anyhow, Result};
use parking_lot::{Condvar, Mutex};
use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tiny_http::{Header, Method, Response, Server};

const MAX_LONG_POLLS: usize = 32;

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
    accept_handle: Option<JoinHandle<()>>,
    poll_handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
    pub port: u16,
}

impl CounterServer {
    pub fn start(port: u16) -> Result<Self> {
        let addr = (IpAddr::V4(Ipv4Addr::LOCALHOST), port);
        let server = Arc::new(Server::http(addr).map_err(|e| anyhow!("http listen {port}: {e}"))?);
        let actual_port = server
            .server_addr()
            .to_ip()
            .map(|addr| addr.port())
            .unwrap_or(port);
        let state = Arc::new((Mutex::new(OverlayState::default()), Condvar::new()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let poll_handles = Arc::new(Mutex::new(Vec::new()));
        let srv = Arc::clone(&server);
        let st = Arc::clone(&state);
        let sd = Arc::clone(&shutdown);
        let ph = Arc::clone(&poll_handles);
        let accept_handle = thread::Builder::new()
            .name("shiny-counter-http".into())
            .spawn(move || run(srv, st, sd, ph))?;

        Ok(Self {
            state,
            shutdown,
            accept_handle: Some(accept_handle),
            poll_handles,
            port: actual_port,
        })
    }

    pub fn update(&self, count: u32, preset: String, armed: bool, styled: bool) {
        let (lock, cvar) = &*self.state;
        let mut s = lock.lock();
        if s.count == count && s.preset == preset && s.armed == armed && s.styled == styled {
            return;
        }
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
        // Change the predicate while holding the mutex paired with the
        // condvar. This makes the predicate-check/wait transition atomic and
        // prevents a poll from missing the shutdown notification and keeping
        // application shutdown blocked for the full 25-second timeout.
        let (lock, cvar) = &*self.state;
        {
            let _guard = lock.lock();
            self.shutdown.store(true, Ordering::Release);
            cvar.notify_all();
        }
        if let Some(handle) = self.accept_handle.take() {
            let _ = handle.join();
        }
        for handle in self.poll_handles.lock().drain(..) {
            let _ = handle.join();
        }
    }
}

fn run(
    server: Arc<Server>,
    state: Arc<(Mutex<OverlayState>, Condvar)>,
    shutdown: Arc<AtomicBool>,
    poll_handles: Arc<Mutex<Vec<JoinHandle<()>>>>,
) {
    let active_polls = Arc::new(AtomicUsize::new(0));
    while !shutdown.load(Ordering::Acquire) {
        reap_finished_polls(&poll_handles);
        match server.recv_timeout(Duration::from_millis(200)) {
            Ok(Some(req)) if is_long_poll(&req) => {
                let previous = active_polls.fetch_add(1, Ordering::Relaxed);
                if previous >= MAX_LONG_POLLS {
                    active_polls.fetch_sub(1, Ordering::Relaxed);
                    let _ = req.respond(
                        Response::from_string("too many long polls").with_status_code(429),
                    );
                    continue;
                }

                let poll_state = Arc::clone(&state);
                let poll_shutdown = Arc::clone(&shutdown);
                let poll_count = Arc::clone(&active_polls);
                match thread::Builder::new()
                    .name("shiny-counter-poll".into())
                    .spawn(move || {
                        handle_request(req, &poll_state, &poll_shutdown);
                        poll_count.fetch_sub(1, Ordering::Relaxed);
                    }) {
                    Ok(handle) => poll_handles.lock().push(handle),
                    Err(_) => {
                        active_polls.fetch_sub(1, Ordering::Relaxed);
                    }
                }
            }
            Ok(Some(req)) => handle_request(req, &state, &shutdown),
            Ok(None) => continue,
            Err(_) => break,
        }
    }
    reap_finished_polls(&poll_handles);
}

fn is_long_poll(req: &tiny_http::Request) -> bool {
    matches!(req.method(), Method::Get) && req.url().split('?').next() == Some("/poll")
}

fn reap_finished_polls(handles: &Mutex<Vec<JoinHandle<()>>>) {
    let finished = {
        let mut handles = handles.lock();
        let mut finished = Vec::new();
        let mut index = 0;
        while index < handles.len() {
            if handles[index].is_finished() {
                finished.push(handles.swap_remove(index));
            } else {
                index += 1;
            }
        }
        finished
    };
    for handle in finished {
        let _ = handle.join();
    }
}

fn handle_request(
    req: tiny_http::Request,
    state: &Arc<(Mutex<OverlayState>, Condvar)>,
    shutdown: &Arc<AtomicBool>,
) {
    if !request_host_is_local(&req) {
        let _ = req.respond(Response::from_string("local host required").with_status_code(421));
        return;
    }
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
            let resp = decorate_response(Response::from_string(body), b"text/plain; charset=utf-8");
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
                // Wait only when the client has exactly the current version.
                // Older and future values both receive the current snapshot
                // immediately, which also recovers from a server restart.
                // `while` not `if`: handles spurious wakeups from the OS as
                // well as the `notify_all` we send from `Drop` on shutdown.
                // Without this, a spurious wake returns the same version the
                // client already has and the JS spins re-polling immediately.
                let deadline = std::time::Instant::now() + Duration::from_secs(25);
                while guard.version == raw_since && !shutdown.load(Ordering::Acquire) {
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
            let resp = decorate_response(Response::from_string(body), b"text/plain; charset=utf-8");
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
            let resp = decorate_response(Response::from_string(body), b"text/html; charset=utf-8");
            let _ = req.respond(resp);
        }
        _ => {
            let _ = req.respond(Response::from_string("not found").with_status_code(404));
        }
    }
}

fn request_host_is_local(req: &tiny_http::Request) -> bool {
    req.headers()
        .iter()
        .find(|header| header.field.equiv("Host"))
        .is_some_and(|header| host_value_is_local(header.value.as_str()))
}

fn host_value_is_local(value: &str) -> bool {
    let value = value.trim();
    let host = if let Some(bracketed) = value.strip_prefix('[') {
        bracketed.split_once(']').map(|(host, _)| host)
    } else if value.matches(':').count() <= 1 {
        Some(value.split_once(':').map_or(value, |(host, _)| host))
    } else {
        None
    };
    host.is_some_and(|host| {
        host.eq_ignore_ascii_case("localhost")
            || host
                .parse::<IpAddr>()
                .is_ok_and(|address| address.is_loopback())
    })
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

fn decorate_response<R: std::io::Read>(
    mut response: Response<R>,
    content_type: &[u8],
) -> Response<R> {
    for (name, value) in [
        (b"Content-Type".as_slice(), content_type),
        (
            b"Cache-Control".as_slice(),
            b"no-store, no-cache, must-revalidate".as_slice(),
        ),
    ] {
        if let Ok(header) = Header::from_bytes(name, value) {
            response.add_header(header);
        }
    }
    response
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
        assert!(
            !body
                .to_ascii_lowercase()
                .contains("access-control-allow-origin"),
            "the local API should not be readable cross-origin"
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

    #[test]
    fn unchanged_updates_do_not_advance_the_poll_version() {
        let srv = CounterServer::start(0).expect("server should bind an ephemeral port");

        srv.update(42, "Test".to_string(), true, true);
        let first_version = srv.state.0.lock().version;
        srv.update(42, "Test".to_string(), true, true);

        assert_eq!(srv.state.0.lock().version, first_version);
    }

    #[test]
    fn count_remains_available_while_a_long_poll_is_waiting() {
        let srv = CounterServer::start(0).expect("server should bind an ephemeral port");
        let port = srv.port;
        srv.update(7, "Test".to_string(), true, true);
        let version = srv.state.0.lock().version;

        let poll = std::thread::spawn(move || {
            ureq_get(&format!("http://127.0.0.1:{port}/poll?since={version}"))
        });
        std::thread::sleep(Duration::from_millis(50));

        let count_response = ureq_get(&format!("http://127.0.0.1:{port}/count"));
        srv.update(8, "Test".to_string(), true, true);
        let _ = poll.join();

        assert!(
            count_response
                .as_deref()
                .is_some_and(|body| body.lines().last().is_some_and(|line| line.trim() == "7")),
            "the regular endpoint was blocked by a pending long poll"
        );
    }

    #[test]
    fn count_remains_available_when_every_long_poll_slot_is_waiting() {
        let srv = CounterServer::start(0).expect("server should bind an ephemeral port");
        let port = srv.port;
        srv.update(7, "Test".to_string(), true, true);
        let version = srv.state.0.lock().version;

        let polls: Vec<_> = (0..MAX_LONG_POLLS)
            .map(|_| {
                std::thread::spawn(move || {
                    ureq_get(&format!("http://127.0.0.1:{port}/poll?since={version}"))
                })
            })
            .collect();
        std::thread::sleep(Duration::from_millis(100));

        let count_response = ureq_get(&format!("http://127.0.0.1:{port}/count"));
        srv.update(8, "Test".to_string(), true, true);
        for poll in polls {
            let _ = poll.join();
        }

        assert!(
            count_response
                .as_deref()
                .is_some_and(|body| body.lines().last().is_some_and(|line| line.trim() == "7")),
            "the regular endpoint was blocked when all long-poll workers were occupied"
        );
    }

    #[test]
    fn future_poll_version_returns_the_current_snapshot_immediately() {
        let srv = CounterServer::start(0).expect("server should bind an ephemeral port");
        let port = srv.port;
        srv.update(7, "Test".to_string(), true, true);
        let started = std::time::Instant::now();

        let response = ureq_get(&format!("http://127.0.0.1:{port}/poll?since=999"))
            .expect("future version should not wait for the long-poll timeout");

        assert!(started.elapsed() < Duration::from_millis(250));
        assert_eq!(
            parse_poll_body(response.lines().last().unwrap_or("")),
            (1, 7)
        );
    }

    #[test]
    fn dropping_server_wakes_a_pending_long_poll_promptly() {
        let srv = CounterServer::start(0).expect("server should bind an ephemeral port");
        let port = srv.port;
        srv.update(7, "Test".to_string(), true, true);
        let version = srv.state.0.lock().version;

        let poll = std::thread::spawn(move || {
            ureq_get(&format!("http://127.0.0.1:{port}/poll?since={version}"))
        });
        std::thread::sleep(Duration::from_millis(50));
        let started = std::time::Instant::now();

        drop(srv);
        let _ = poll.join();

        assert!(
            started.elapsed() < Duration::from_secs(1),
            "server shutdown waited for the long-poll timeout"
        );
    }

    #[test]
    fn requests_with_a_non_local_host_are_rejected() {
        let srv = CounterServer::start(0).expect("server should bind an ephemeral port");
        let response = ureq_get_with_host(
            &format!("http://127.0.0.1:{}/count", srv.port),
            "attacker.example",
        )
        .expect("server should return a rejection response");

        assert!(response.starts_with("HTTP/1.1 421"), "got: {response:?}");
        assert!(!response.lines().last().is_some_and(|line| line == "0"));
    }

    #[test]
    fn host_validation_accepts_only_loopback_names_and_addresses() {
        for host in [
            "localhost",
            "localhost:7878",
            "127.0.0.1",
            "127.0.0.1:7878",
            "[::1]:7878",
        ] {
            assert!(host_value_is_local(host), "should accept {host}");
        }
        for host in [
            "example.com",
            "localhost.example.com",
            "127.0.0.1.example.com",
            "192.168.1.2:7878",
            "::1:7878",
        ] {
            assert!(!host_value_is_local(host), "should reject {host}");
        }
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
        let (_scheme, rest) = url.split_once("://")?;
        let host = rest.split('/').next()?;
        ureq_get_with_host(url, host)
    }

    fn ureq_get_with_host(url: &str, host_header: &str) -> Option<String> {
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
            "GET {path_and_query} HTTP/1.1\r\nHost: {host_header}\r\nConnection: close\r\n\r\n"
        )
        .ok()?;
        let mut buf = String::new();
        stream.read_to_string(&mut buf).ok()?;
        Some(buf)
    }
}
