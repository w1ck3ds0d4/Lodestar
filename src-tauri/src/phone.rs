//! Local LAN companion server for phone control.
//!
//! A phone on the same Wi-Fi opens `http://<pc-ip>:8420`, enters a PIN, and gets a
//! page that shows the live transcript + answers and can drive the assistant (start/
//! stop listening, answer the screen, ask a question). It is a thin relay:
//!  - the desktop UI pushes state to phones via `tx` (broadcast),
//!  - phone commands arrive over the socket and are re-emitted to the desktop UI as
//!    `phone-command` events, which the existing frontend handlers execute.
//!
//! PIN-gated and LAN-only. Traffic is plain HTTP/WS (no TLS) - intended for a trusted
//! personal network; the PIN is the gate, not transport security.

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::{Html, IntoResponse};
use axum::routing::get;
use axum::Router;
use base64::Engine;
use rand::Rng;
use tauri::{AppHandle, Emitter};
use tokio::sync::{broadcast, oneshot};

pub const PORT: u16 = 8420;
const PAGE: &str = include_str!("phone.html");

/// Handle to a running companion server, stored in `AppState`.
pub struct PhoneServer {
    pub pin: String,
    pub url: String,
    tx: broadcast::Sender<String>,
    shutdown: oneshot::Sender<()>,
}

impl PhoneServer {
    /// Push a JSON state message to all connected phones (no-op if none).
    pub fn broadcast(&self, msg: &str) {
        let _ = self.tx.send(msg.to_string());
    }
    pub fn stop(self) {
        let _ = self.shutdown.send(());
    }
}

#[derive(Clone)]
struct ServerState {
    app: AppHandle,
    tx: broadcast::Sender<String>,
    pin: String,
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

/// Start the companion server. Returns (url, pin, qr-data-url). Binds 0.0.0.0:8420.
pub fn start(app: AppHandle) -> Result<PhoneServer, String> {
    let ip =
        local_ip_address::local_ip().map_err(|e| format!("Could not find your LAN IP: {e}"))?;
    let url = format!("http://{ip}:{PORT}");
    let pin = format!("{:06}", rand::rng().random_range(0..1_000_000));

    // Bind synchronously so a port-in-use error is reported to the caller now.
    let std_listener = std::net::TcpListener::bind(("0.0.0.0", PORT))
        .map_err(|e| format!("Could not start the phone server on port {PORT}: {e}"))?;
    std_listener
        .set_nonblocking(true)
        .map_err(|e| e.to_string())?;

    let (tx, _rx) = broadcast::channel::<String>(128);
    let (shutdown, shutdown_rx) = oneshot::channel::<()>();
    let state = ServerState {
        app: app.clone(),
        tx: tx.clone(),
        pin: pin.clone(),
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
                let _ = shutdown_rx.await;
            });
        if let Err(e) = server.await {
            tracing::warn!("phone server stopped: {e}");
        }
    });

    Ok(PhoneServer {
        pin,
        url,
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

async fn page() -> Html<&'static str> {
    Html(PAGE)
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(st): State<ServerState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, st))
}

async fn handle_socket(mut socket: WebSocket, st: ServerState) {
    // The phone must send {"pin":"..."} as its first message.
    let authed = matches!(socket.recv().await, Some(Ok(Message::Text(t)))
        if serde_json::from_str::<serde_json::Value>(&t)
            .ok()
            .and_then(|v| v.get("pin").and_then(|p| p.as_str()).map(|p| p == st.pin))
            .unwrap_or(false));
    let ack = if authed {
        r#"{"type":"auth","ok":true}"#
    } else {
        r#"{"type":"auth","ok":false}"#
    };
    let _ = socket.send(Message::Text(ack.to_string().into())).await;
    if !authed {
        return;
    }

    let mut rx = st.tx.subscribe();
    loop {
        tokio::select! {
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
