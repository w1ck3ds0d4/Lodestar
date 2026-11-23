//! Local LAN companion server for phone control.
//!
//! A phone on the same Wi-Fi opens `http://<pc-ip>:8420`, enters a PIN, and gets a
//! page that shows the live transcript + answers and can drive the assistant (start/
//! stop listening, answer the screen, ask a question). It is a thin relay:
//!  - the desktop UI pushes state to phones via `tx` (broadcast),
//!  - phone commands arrive over the socket and are re-emitted to the desktop UI as
//!    `phone-command` events, which the existing frontend handlers execute.
//!
//! Opt-in (the user starts it from Settings) and gated by a 6-digit PIN. The listener
//! binds every interface (0.0.0.0) because the phone reaches it over the LAN; the
//! Windows Firewall rule added by `netcheck.rs` scopes inbound traffic to the local
//! subnet, but this module does not enforce that itself. Traffic is plain HTTP/WS (no
//! TLS), so anyone on the same network can read it: use a network you trust.
//!
//! Guessing protection (see `PinGuard`):
//!  - at most `MAX_FAILS_PER_PIN` wrong attempts per PIN, then the PIN is replaced and
//!    the desktop shows the new one;
//!  - after `MAX_ROTATIONS` replacements without a successful pairing the server stops
//!    and the desktop says why; the user has to start it again;
//!  - every wrong attempt is answered after `FAIL_DELAY`;
//!  - the Host header must be one of the server's own addresses (its LAN `ip:port`,
//!    `localhost` or `127.0.0.1`) on both the page and the socket, so a DNS-rebinding
//!    page cannot reach it under its own name (see `host_allowed`);
//!  - a WebSocket upgrade from a browser page on another origin is refused (see
//!    `origin_allowed`). Non-browser clients send no Origin and still face the PIN;
//!  - silent sockets and frames that are not a PIN attempt are dropped without being
//!    counted as wrong PINs, so they cannot burn the budget.
//!
//! The wrong-attempt budget is shared by everyone who can reach the port, so any host on
//! the LAN can make the server stop itself by guessing wrong. That is a nuisance, not a
//! way in: the user starts the server again and gets a new PIN.
//!
//! This is a brute-force limiter, not authentication of the network: the PIN travels in
//! clear text on the LAN and does not defend against someone who can observe traffic.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use base64::Engine;
use rand::RngExt;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::{broadcast, watch};

pub const PORT: u16 = 8420;
const PAGE: &str = include_str!("phone.html");

/// Wrong attempts allowed against one PIN before it is replaced.
const MAX_FAILS_PER_PIN: u32 = 5;
/// PIN replacements without a successful pairing before the server stops.
const MAX_ROTATIONS: u32 = 3;
/// Pause before a wrong-PIN answer goes back, to slow guessing further.
const FAIL_DELAY: Duration = Duration::from_millis(750);
/// Tries (and pause between them) to bind the port when it is still held by a server
/// that was just stopped.
const BIND_RETRIES: u32 = 10;
const BIND_RETRY_PAUSE: Duration = Duration::from_millis(100);
/// How long a new socket may stay silent before it is dropped.
const AUTH_TIMEOUT: Duration = Duration::from_secs(10);

/// What a PIN attempt did to the guard.
#[derive(Debug, PartialEq, Eq)]
enum PinOutcome {
    /// Correct PIN. The replacement count is cleared; wrong attempts on this PIN stay.
    Accepted,
    /// Wrong PIN, attempts remain on the current one.
    Rejected,
    /// Wrong PIN and that was the last allowed attempt: the PIN was replaced.
    Rotated,
    /// Wrong PIN (or any PIN) and the server is locked: it must be stopped.
    Stopped,
}

/// Brute-force bookkeeping for the pairing PIN. Pure state, no I/O, so the limits are
/// unit-testable. Every attempt goes through `check` under one lock, so concurrent
/// sockets cannot exceed the per-PIN budget.
struct PinGuard {
    pin: String,
    fails: u32,
    rotations: u32,
    stopped: bool,
}

impl PinGuard {
    fn new(pin: String) -> Self {
        Self {
            pin,
            fails: 0,
            rotations: 0,
            stopped: false,
        }
    }

    /// Judge one attempt. `next_pin` supplies the replacement PIN when this attempt
    /// uses up the budget of the current one.
    fn check(&mut self, attempt: &str, next_pin: impl FnOnce(&str) -> String) -> PinOutcome {
        if self.stopped {
            return PinOutcome::Stopped;
        }
        if attempt == self.pin {
            // `fails` is kept: the PIN is unchanged, so a pairing must not hand out a
            // fresh budget against it. Only the replacement count is cleared.
            self.rotations = 0;
            return PinOutcome::Accepted;
        }
        self.fails += 1;
        if self.fails < MAX_FAILS_PER_PIN {
            return PinOutcome::Rejected;
        }
        self.fails = 0;
        self.rotations += 1;
        if self.rotations >= MAX_ROTATIONS {
            self.stopped = true;
            return PinOutcome::Stopped;
        }
        self.pin = next_pin(&self.pin);
        PinOutcome::Rotated
    }
}

/// A fresh random 6-digit PIN that differs from `old`.
fn new_pin(old: &str) -> String {
    loop {
        let pin = format!("{:06}", rand::rng().random_range(0..1_000_000));
        if pin != old {
            return pin;
        }
    }
}

/// The Host header values this server answers to: its own LAN address plus the loopback
/// names, each with the port. Anything else (an attacker's name rebound to this machine)
/// is refused, so the Origin check below cannot be satisfied by an attacker-chosen Host.
fn allowed_hosts(url: &str) -> Vec<String> {
    let mut hosts = Vec::new();
    if let Some(own) = url.strip_prefix("http://") {
        hosts.push(own.to_string());
    }
    for name in ["localhost", "127.0.0.1", "[::1]"] {
        hosts.push(format!("{name}:{PORT}"));
    }
    hosts
}

/// Whether the request's Host header names this server (case-insensitive). A missing
/// Host is refused.
fn host_allowed(host: Option<&str>, allowed: &[String]) -> bool {
    host.is_some_and(|h| allowed.iter().any(|a| a.eq_ignore_ascii_case(h)))
}

/// Whether a WebSocket upgrade may proceed. A browser always sends an Origin; it must be
/// one of the server's own origins (`http://` plus an allowed host, so scheme, host and
/// port all match), never a value echoed from the request. That stops a web page on
/// another site from driving the socket from a visitor's browser. A missing Origin means
/// a non-browser client and goes on to the PIN check. The server only speaks plain HTTP,
/// so the scheme is always `http`.
fn origin_allowed(origin: Option<&str>, allowed: &[String]) -> bool {
    let Some(origin) = origin else {
        return true;
    };
    allowed
        .iter()
        .any(|a| origin.eq_ignore_ascii_case(&format!("http://{a}")))
}

/// Payload of the `phone-rotated` event: the connection details with the new PIN.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct RotatedEvent {
    url: String,
    pin: String,
    qr: String,
}

/// Handle to a running companion server, stored in `AppState`.
pub struct PhoneServer {
    pub url: String,
    guard: Arc<Mutex<PinGuard>>,
    tx: broadcast::Sender<String>,
    shutdown: watch::Sender<bool>,
}

impl PhoneServer {
    /// The PIN currently accepted (it changes after repeated wrong attempts).
    pub fn pin(&self) -> String {
        self.guard.lock().map(|g| g.pin.clone()).unwrap_or_default()
    }
    /// True once too many wrong PINs shut the server down.
    pub fn is_stopped(&self) -> bool {
        self.guard.lock().map(|g| g.stopped).unwrap_or(true)
    }
    /// Push a JSON state message to all connected phones (no-op if none).
    pub fn broadcast(&self, msg: &str) {
        let _ = self.tx.send(msg.to_string());
    }
    pub fn stop(self) {
        let _ = self.shutdown.send(true);
    }
}

#[derive(Clone)]
struct ServerState {
    app: AppHandle,
    tx: broadcast::Sender<String>,
    url: String,
    hosts: Arc<Vec<String>>,
    guard: Arc<Mutex<PinGuard>>,
    shutdown: watch::Sender<bool>,
}

/// A QR code (SVG, as a data: URL) of the connect URL, for the phone to scan.
fn qr_data_url(url: &str) -> String {
    match qrcode::QrCode::new(url.as_bytes()) {
        Ok(code) => {
            let svg = code
                .render::<qrcode::render::svg::Color>()
                .min_dimensions(180, 180)
                .quiet_zone(true)
                .build();
            let b64 = base64::engine::general_purpose::STANDARD.encode(svg.as_bytes());
            format!("data:image/svg+xml;base64,{b64}")
        }
        Err(_) => String::new(),
    }
}

/// Bind 0.0.0.0:8420. A server that was just stopped may still be closing its listener,
/// so an address-in-use error is retried for a moment before it is reported.
fn bind_with_retry() -> Result<std::net::TcpListener, String> {
    let mut attempts = 0;
    loop {
        match std::net::TcpListener::bind(("0.0.0.0", PORT)) {
            Ok(l) => return Ok(l),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse && attempts < BIND_RETRIES => {
                attempts += 1;
                std::thread::sleep(BIND_RETRY_PAUSE);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                return Err(format!(
                    "Port {PORT} is still in use. Wait a moment and start the phone server again, or close whatever else is using it."
                ));
            }
            Err(e) => {
                return Err(format!(
                    "Could not start the phone server on port {PORT}: {e}"
                ))
            }
        }
    }
}

/// Start the companion server. Returns (url, pin, qr-data-url). Binds 0.0.0.0:8420.
pub fn start(app: AppHandle) -> Result<PhoneServer, String> {
    let ip =
        local_ip_address::local_ip().map_err(|e| format!("Could not find your LAN IP: {e}"))?;
    let url = format!("http://{ip}:{PORT}");
    let pin = new_pin("");

    // Bind synchronously so a port-in-use error is reported to the caller now.
    let std_listener = bind_with_retry()?;
    std_listener
        .set_nonblocking(true)
        .map_err(|e| e.to_string())?;

    let (tx, _rx) = broadcast::channel::<String>(128);
    let (shutdown, mut shutdown_rx) = watch::channel(false);
    let guard = Arc::new(Mutex::new(PinGuard::new(pin)));
    let state = ServerState {
        app: app.clone(),
        tx: tx.clone(),
        url: url.clone(),
        hosts: Arc::new(allowed_hosts(&url)),
        guard: guard.clone(),
        shutdown: shutdown.clone(),
    };
    let router = Router::new()
        .route("/", get(page))
        .route("/ws", get(ws_upgrade))
        .with_state(state);

    tauri::async_runtime::spawn(async move {
        let listener = match tokio::net::TcpListener::from_std(std_listener) {
            Ok(l) => l,
            Err(e) => {
                tracing::warn!("phone server listener failed: {e}");
                return;
            }
        };
        let server =
            axum::serve(listener, router.into_make_service()).with_graceful_shutdown(async move {
                let _ = shutdown_rx.wait_for(|stop| *stop).await;
            });
        if let Err(e) = server.await {
            tracing::warn!("phone server stopped: {e}");
        }
    });

    Ok(PhoneServer {
        url,
        guard,
        tx,
        shutdown,
    })
}

/// The connect URL's QR as a data URL (recomputed for the desktop UI). The PIN is
/// carried in the URL fragment so scanning the code connects without typing it; the
/// fragment is never sent to the server, so it stays out of request logs.
pub fn qr_for(url: &str, pin: &str) -> String {
    qr_data_url(&format!("{url}/#pin={pin}"))
}

async fn page(headers: HeaderMap, State(st): State<ServerState>) -> Response {
    if !host_allowed(header_text(&headers, header::HOST), &st.hosts) {
        return (StatusCode::FORBIDDEN, "Unknown host").into_response();
    }
    Html(PAGE).into_response()
}

fn header_text(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

async fn ws_upgrade(
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    State(st): State<ServerState>,
) -> impl IntoResponse {
    if !host_allowed(header_text(&headers, header::HOST), &st.hosts) {
        return (StatusCode::FORBIDDEN, "Unknown host").into_response();
    }
    if !origin_allowed(header_text(&headers, header::ORIGIN), &st.hosts) {
        return (StatusCode::FORBIDDEN, "Cross-origin connection refused").into_response();
    }
    ws.on_upgrade(move |socket| handle_socket(socket, st))
        .into_response()
}

/// Read the PIN from the phone's first message. None if it is missing or malformed.
async fn read_pin(socket: &mut WebSocket) -> Option<String> {
    let first = tokio::time::timeout(AUTH_TIMEOUT, socket.recv())
        .await
        .ok()??;
    let Ok(Message::Text(t)) = first else {
        return None;
    };
    let v: serde_json::Value = serde_json::from_str(&t).ok()?;
    Some(v.get("pin")?.as_str()?.to_string())
}

async fn handle_socket(mut socket: WebSocket, st: ServerState) {
    // The phone must send {"pin":"..."} as its first message.
    let Some(attempt) = read_pin(&mut socket).await else {
        // Silent or malformed: not a guess, so it does not use up the budget.
        tokio::time::sleep(FAIL_DELAY).await;
        let _ = socket
            .send(Message::Text(
                r#"{"type":"auth","ok":false}"#.to_string().into(),
            ))
            .await;
        return;
    };
    let outcome = match st.guard.lock() {
        Ok(mut g) => g.check(&attempt, new_pin),
        Err(_) => PinOutcome::Stopped,
    };
    if outcome != PinOutcome::Accepted {
        tokio::time::sleep(FAIL_DELAY).await;
    }
    let ack = match outcome {
        PinOutcome::Accepted => r#"{"type":"auth","ok":true}"#,
        PinOutcome::Rejected => r#"{"type":"auth","ok":false}"#,
        PinOutcome::Rotated => r#"{"type":"auth","ok":false,"rotated":true}"#,
        PinOutcome::Stopped => r#"{"type":"auth","ok":false,"locked":true}"#,
    };
    let _ = socket.send(Message::Text(ack.to_string().into())).await;
    match outcome {
        PinOutcome::Accepted => {}
        PinOutcome::Rejected => return,
        PinOutcome::Rotated => {
            let pin = st.guard.lock().map(|g| g.pin.clone()).unwrap_or_default();
            let qr = qr_for(&st.url, &pin);
            let _ = st.app.emit(
                "phone-rotated",
                RotatedEvent {
                    url: st.url.clone(),
                    pin,
                    qr,
                },
            );
            return;
        }
        PinOutcome::Stopped => {
            tracing::warn!("phone server stopped after repeated wrong PINs");
            let _ = st.app.emit(
                "phone-stopped",
                "Too many wrong PINs, so the phone server was stopped. Start it again to get a new PIN.",
            );
            let _ = st.shutdown.send(true);
            return;
        }
    }

    let mut rx = st.tx.subscribe();
    let mut stop_rx = st.shutdown.subscribe();
    loop {
        tokio::select! {
            _ = async { let _ = stop_rx.wait_for(|stop| *stop).await; } => break,
            outbound = rx.recv() => match outbound {
                Ok(msg) => {
                    if socket.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Closed) => break,
                Err(broadcast::error::RecvError::Lagged(_)) => {}
            },
            inbound = socket.recv() => match inbound {
                // Phone commands are relayed to the desktop UI, which executes them.
                Some(Ok(Message::Text(t))) => {
                    let _ = st.app.emit("phone-command", t.to_string());
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(_)) => break,
                _ => {}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pins() -> impl FnMut(&str) -> String {
        let mut n = 0;
        move |_| {
            n += 1;
            format!("{n:06}")
        }
    }

    #[test]
    fn five_wrong_attempts_then_the_pin_rotates() {
        let mut g = PinGuard::new("123456".into());
        let mut next = pins();
        for _ in 0..MAX_FAILS_PER_PIN - 1 {
            assert_eq!(g.check("000000", &mut next), PinOutcome::Rejected);
            assert_eq!(g.pin, "123456");
        }
        assert_eq!(g.check("000000", &mut next), PinOutcome::Rotated);
        assert_eq!(g.pin, "000001", "the PIN is replaced after the 5th failure");
        // The old PIN no longer works and does not count as a success.
        assert_eq!(g.check("123456", &mut next), PinOutcome::Rejected);
        assert_eq!(g.check("000001", &mut next), PinOutcome::Accepted);
    }

    #[test]
    fn three_rotations_without_success_stop_the_server() {
        let mut g = PinGuard::new("123456".into());
        let mut next = pins();
        for round in 1..=MAX_ROTATIONS {
            for _ in 0..MAX_FAILS_PER_PIN - 1 {
                assert_eq!(g.check("x", &mut next), PinOutcome::Rejected);
            }
            let expect = if round < MAX_ROTATIONS {
                PinOutcome::Rotated
            } else {
                PinOutcome::Stopped
            };
            assert_eq!(g.check("x", &mut next), expect, "round {round}");
        }
        assert!(g.stopped);
        // Once stopped even the right PIN is refused.
        let right = g.pin.clone();
        assert_eq!(g.check(&right, &mut next), PinOutcome::Stopped);
    }

    #[test]
    fn a_success_resets_the_rotation_count() {
        let mut g = PinGuard::new("123456".into());
        let mut next = pins();
        for _ in 0..MAX_FAILS_PER_PIN {
            g.check("x", &mut next);
        }
        assert_eq!(g.rotations, 1);
        let pin = g.pin.clone();
        assert_eq!(g.check(&pin, &mut next), PinOutcome::Accepted);
        assert_eq!(g.rotations, 0);
        assert!(!g.stopped);
    }

    #[test]
    fn new_pin_is_six_digits_and_differs() {
        for _ in 0..50 {
            let p = new_pin("123456");
            assert_eq!(p.len(), 6);
            assert!(p.bytes().all(|b| b.is_ascii_digit()));
            assert_ne!(p, "123456");
        }
    }

    fn hosts() -> Vec<String> {
        allowed_hosts("http://192.168.1.20:8420")
    }

    #[test]
    fn missing_origin_is_allowed_through_to_the_pin_check() {
        assert!(origin_allowed(None, &hosts()));
    }

    #[test]
    fn own_origins_are_allowed() {
        let h = hosts();
        assert!(origin_allowed(Some("http://192.168.1.20:8420"), &h));
        assert!(origin_allowed(Some("http://localhost:8420"), &h));
        assert!(origin_allowed(Some("http://LOCALHOST:8420"), &h));
        assert!(origin_allowed(Some("http://127.0.0.1:8420"), &h));
    }

    #[test]
    fn foreign_origins_are_refused() {
        let h = hosts();
        assert!(!origin_allowed(Some("http://evil.example"), &h));
        assert!(!origin_allowed(Some("http://evil.example:8420"), &h));
        assert!(!origin_allowed(Some("https://192.168.1.20:8420"), &h));
        assert!(!origin_allowed(Some("http://192.168.1.20:9999"), &h));
        assert!(!origin_allowed(Some("http://192.168.1.20"), &h));
        assert!(!origin_allowed(Some("null"), &h));
    }

    #[test]
    fn own_hosts_are_allowed_and_others_are_not() {
        let h = hosts();
        assert!(host_allowed(Some("192.168.1.20:8420"), &h));
        assert!(host_allowed(Some("localhost:8420"), &h));
        assert!(host_allowed(Some("127.0.0.1:8420"), &h));
        assert!(host_allowed(Some("[::1]:8420"), &h));
        assert!(!host_allowed(None, &h));
        assert!(!host_allowed(Some("evil.example:8420"), &h));
        assert!(!host_allowed(Some("192.168.1.20"), &h));
        assert!(!host_allowed(Some("192.168.1.21:8420"), &h));
    }

    #[test]
    fn dns_rebinding_with_matching_host_and_origin_is_refused() {
        // The attacker page is served from evil.example:8420 and the name is rebound to
        // this machine, so Host and Origin agree with each other but not with the server.
        let h = hosts();
        assert!(!host_allowed(Some("evil.example:8420"), &h));
        assert!(!origin_allowed(Some("http://evil.example:8420"), &h));
    }

    #[test]
    fn a_success_keeps_the_wrong_attempts_on_the_same_pin() {
        let mut g = PinGuard::new("123456".into());
        let mut next = pins();
        for _ in 0..4 {
            g.check("0", &mut next);
        }
        assert_eq!(g.check("123456", &mut next), PinOutcome::Accepted);
        assert_eq!(g.fails, 4, "pairing must not refill the budget");
        assert_eq!(g.check("0", &mut next), PinOutcome::Rotated);
    }
}
