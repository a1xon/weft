//! Loopback-only HTTP + WebSocket front for the engine. Every request must
//! carry the per-daemon token and a loopback Host header, so a web page in
//! your browser cannot drive your agents (CSRF / DNS rebinding).

use crate::engine::{Cmd, Ev};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::{header, HeaderMap, StatusCode},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
pub struct App {
    pub eng: mpsc::UnboundedSender<Ev>,
    pub token: String,
    pub port: u16,
    pub next: Arc<AtomicU64>,
    /// `<project>/.weft/shots`: the only folder images are read from or written to
    pub shots: std::path::PathBuf,
}

pub(crate) fn host_ok(h: &HeaderMap, port: u16) -> bool {
    let Some(v) = h.get(header::HOST).and_then(|v| v.to_str().ok()) else { return false };
    v == format!("127.0.0.1:{port}") || v == format!("localhost:{port}") || v == format!("[::1]:{port}")
}

fn token_ok(h: &HeaderMap, q: &HashMap<String, String>, app: &App) -> bool {
    let given = h.get("x-weft-token").and_then(|v| v.to_str().ok()).or_else(|| q.get("t").map(|s| s.as_str())).unwrap_or("");
    // constant-time-ish compare; the token is per-daemon and loopback-only anyway
    given.len() == app.token.len() && given.bytes().zip(app.token.bytes()).fold(0u8, |a, (x, y)| a | (x ^ y)) == 0
}

fn origin_ok(h: &HeaderMap, port: u16) -> bool {
    match h.get(header::ORIGIN).and_then(|v| v.to_str().ok()) {
        None => true,
        Some(o) => o == format!("http://127.0.0.1:{port}") || o == format!("http://localhost:{port}"),
    }
}

pub fn router(app: App) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/ws", get(ws_upgrade))
        .route("/f/{name}", get(font))
        .route("/shots/{name}", get(shot))
        .route("/api/shot", post(upload_shot).layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024)))
        // (cards handed over by another board carry all they knew: see engine/moves.rs)
        .route("/api/cmd", post(api_cmd).layer(axum::extract::DefaultBodyLimit::max(16 * 1024 * 1024)))
        .merge(crate::dev::routes()) // dev: the page follows edits to web/ (src/dev.rs)
        .with_state(app)
}

async fn index(State(app): State<App>, h: HeaderMap) -> Response {
    if !host_ok(&h, app.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    // dev: from web/ on disk while weft's own UI is being worked on (src/dev.rs)
    let body = crate::dev::page(crate::model::BUILD).unwrap_or_else(|| crate::web::page(crate::model::BUILD));
    ([(header::CACHE_CONTROL, "no-store")], Html(body)).into_response()
}

async fn font(State(app): State<App>, h: HeaderMap, axum::extract::Path(name): axum::extract::Path<String>) -> Response {
    if !host_ok(&h, app.port) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match crate::web::FONTS.iter().find(|(n, _)| *n == name) {
        Some((_, bytes)) => {
            ([(header::CONTENT_TYPE, "font/woff2"), (header::CACHE_CONTROL, "public, max-age=31536000, immutable")], *bytes).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Only plain file names, only image extensions: nothing outside the shots folder is reachable.
fn safe_image_name(name: &str) -> Option<&'static str> {
    if name.is_empty() || name.len() > 120 || !name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) || name.starts_with('.')
    {
        return None;
    }
    match name.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

async fn shot(
    State(app): State<App>,
    h: HeaderMap,
    Query(q): Query<HashMap<String, String>>,
    axum::extract::Path(name): axum::extract::Path<String>,
) -> Response {
    if !host_ok(&h, app.port) || !token_ok(&h, &q, &app) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let Some(ct) = safe_image_name(&name) else { return StatusCode::NOT_FOUND.into_response() };
    match tokio::fs::read(app.shots.join(&name)).await {
        Ok(bytes) => ([(header::CONTENT_TYPE, ct), (header::CACHE_CONTROL, "private, max-age=30")], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// An image the user pasted or dropped on a gate: sniffed, saved, then attached through the engine.
async fn upload_shot(State(app): State<App>, h: HeaderMap, Query(q): Query<HashMap<String, String>>, body: axum::body::Bytes) -> Response {
    if !host_ok(&h, app.port) || !token_ok(&h, &q, &app) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let plain = |g: &&String| !g.is_empty() && g.len() < 16 && g.chars().all(|c| c.is_ascii_alphanumeric());
    // an upload is for a gate (`gate=`) or for the watch on an agent (`node=`)
    let (target, is_node) = match (q.get("gate").filter(plain), q.get("node").filter(plain)) {
        (Some(g), _) => (g.clone(), false),
        (None, Some(n)) => (n.clone(), true),
        _ => return StatusCode::BAD_REQUEST.into_response(),
    };
    let Some(ext) = crate::model::image_ext(&body[..]) else {
        return (StatusCode::UNSUPPORTED_MEDIA_TYPE, "not an image").into_response();
    };
    let b = &body[..];
    let name = format!("{target}-upload-{}.{ext}", crate::engine::now_ms());
    if tokio::fs::create_dir_all(&app.shots).await.is_err() || tokio::fs::write(app.shots.join(&name), b).await.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let cmd = if is_node {
        Cmd::FeedAdd { node: target, name: name.clone(), note: q.get("note").cloned().unwrap_or_default() }
    } else {
        Cmd::GateShot { id: target, name: name.clone() }
    };
    match ask(&app, cmd, Duration::from_secs(5)).await {
        Ok(v) => Json(json!({"ok": v["ok"], "name": name, "error": v["error"]})).into_response(),
        Err(code) => code.into_response(),
    }
}

async fn api_cmd(State(app): State<App>, h: HeaderMap, Query(q): Query<HashMap<String, String>>, Json(cmd): Json<Cmd>) -> Response {
    if !host_ok(&h, app.port) || !token_ok(&h, &q, &app) {
        return StatusCode::FORBIDDEN.into_response();
    }
    match ask(&app, cmd, Duration::from_secs(10)).await {
        Ok(v) => Json(v).into_response(),
        Err(code) => code.into_response(),
    }
}

/// A command from a request, for the engine: its answer (or that none came in time).
async fn ask(app: &App, cmd: Cmd, wait: Duration) -> Result<Value, StatusCode> {
    let (tx, rx) = oneshot::channel();
    app.eng.send(Ev::Cmd { client: None, cmd, reply: Some(tx) }).map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    Ok(match tokio::time::timeout(wait, rx).await {
        Ok(Ok(v)) => v,
        _ => json!({"ok": false, "error": "engine did not answer"}),
    })
}

async fn ws_upgrade(State(app): State<App>, h: HeaderMap, Query(q): Query<HashMap<String, String>>, ws: WebSocketUpgrade) -> Response {
    if !host_ok(&h, app.port) || !origin_ok(&h, app.port) || !token_ok(&h, &q, &app) {
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.on_upgrade(move |sock| conn(sock, app))
}

async fn conn(mut sock: WebSocket, app: App) {
    let id = app.next.fetch_add(1, Ordering::Relaxed);
    // Bounded: a client that cannot keep up is dropped and resyncs on reconnect.
    let (tx, mut rx) = mpsc::channel::<String>(512);
    // Replies to requests that carry an `rid` (e.g. "add node" -> new id).
    let (ack_tx, mut ack_rx) = mpsc::unbounded_channel::<String>();
    if app.eng.send(Ev::Conn { id, tx }).is_err() {
        return;
    }
    loop {
        tokio::select! {
            m = sock.recv() => match m {
                Some(Ok(Message::Text(t))) => {
                    let Ok(v) = serde_json::from_str::<Value>(&t) else { continue };
                    let rid = v.get("rid").and_then(|r| r.as_u64());
                    let cmd = match serde_json::from_value::<Cmd>(v) {
                        Ok(c) => c,
                        Err(e) => {
                            // the page is waiting for an answer: say what was wrong instead of letting it time out
                            if let Some(rid) = rid {
                                let r = json!({"ok": false, "error": format!("not understood: {e}")});
                                let _ = ack_tx.send(json!({"t": "ack", "rid": rid, "r": r}).to_string());
                            }
                            continue;
                        }
                    };
                    let reply = rid.map(|rid| {
                        let (otx, orx) = oneshot::channel::<Value>();
                        let ack = ack_tx.clone();
                        tokio::spawn(async move {
                            if let Ok(r) = orx.await {
                                let _ = ack.send(json!({"t": "ack", "rid": rid, "r": r}).to_string());
                            }
                        });
                        otx
                    });
                    if app.eng.send(Ev::Cmd { client: Some(id), cmd, reply }).is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            },
            out = rx.recv() => match out {
                Some(s) => {
                    if sock.send(Message::Text(s.into())).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            Some(a) = ack_rx.recv() => {
                if sock.send(Message::Text(a.into())).await.is_err() {
                    break;
                }
            }
        }
    }
    let _ = app.eng.send(Ev::Disc { id });
}
