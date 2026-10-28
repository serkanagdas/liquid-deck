//! Liquid Deck — a single-binary Stream Deck and live hardware dashboard.
//!
//! The entire frontend is embedded with `rust-embed`; the release binary has no
//! runtime assets. One axum server exposes the UI, a JSON API and a WebSocket
//! metrics stream on the same port.

mod actions;
mod audio;
mod deck;
mod metrics;
mod obs;
mod sounds;

use std::{
    collections::{HashMap, VecDeque},
    env,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        DefaultBodyLimit, Query, Request, State,
    },
    http::{header, HeaderMap, HeaderValue, Method, StatusCode, Uri},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rust_embed::RustEmbed;
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{broadcast, mpsc, Mutex, Notify, RwLock};

use actions::{ActionRequest, Executor};
use deck::{DeckConfig, DeckState};
use metrics::{HistoryPoint, Metrics, HISTORY_LEN, PING_NONE};

const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Maximum actions accepted per second across HTTP and WebSocket.
const ACTION_RATE_LIMIT: u32 = 25;

#[derive(RustEmbed)]
#[folder = "web/"]
struct Assets;

#[derive(Clone)]
struct Config {
    bind: SocketAddr,
    token: Option<String>,
    scripts_dir: Option<PathBuf>,
    sounds_dir: Option<PathBuf>,
    seed_sounds: bool,
    deck_path: PathBuf,
    state_path: PathBuf,
    ping_target: String,
    sample_ms: u64,
    idle_ms: u64,
}

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<Arc<String>>,
    events: broadcast::Sender<Arc<String>>,
    latest: Arc<RwLock<Option<Metrics>>>,
    history: Arc<Mutex<VecDeque<HistoryPoint>>>,
    clients: Arc<AtomicUsize>,
    activity: Arc<AtomicU64>,
    wake: Arc<Notify>,
    executor: Arc<Executor>,
    config: Arc<Config>,
    gate: Arc<Mutex<ActionGate>>,
    deck: Arc<RwLock<DeckConfig>>,
    deck_state: Arc<RwLock<DeckState>>,
}

impl AppState {
    fn authorized(&self, headers: &HeaderMap, query_token: Option<&str>) -> bool {
        let Some(expected) = self.config.token.as_deref() else {
            return true;
        };

        let header_token = headers
            .get("x-liquid-token")
            .and_then(|value| value.to_str().ok())
            .or_else(|| {
                headers
                    .get(header::AUTHORIZATION)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.strip_prefix("Bearer "))
            });

        query_token
            .filter(|token| !token.is_empty())
            .or(header_token)
            .map(|token| constant_time_eq(token.as_bytes(), expected.as_bytes()))
            .unwrap_or(false)
    }

    async fn allow_action(&self) -> bool {
        self.gate.lock().await.allow()
    }
}

struct ActionGate {
    window: Instant,
    count: u32,
}

impl ActionGate {
    fn new() -> Self {
        Self {
            window: Instant::now(),
            count: 0,
        }
    }

    fn allow(&mut self) -> bool {
        if self.window.elapsed() >= Duration::from_secs(1) {
            self.window = Instant::now();
            self.count = 0;
        }
        if self.count >= ACTION_RATE_LIMIT {
            return false;
        }
        self.count += 1;
        true
    }
}

struct ClientGuard(Arc<AtomicUsize>);

impl Drop for ClientGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("liquid-deck: {error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let config = parse_config()?;
    let bind = config.bind;

    let (deck_config, warning) = deck::load_or_default(&config.deck_path);
    if let Some(error) = warning {
        eprintln!("liquid-deck: ignoring {} ({error}); using built-in deck.", config.deck_path.display());
    }
    if !config.deck_path.exists() {
        if let Err(error) = deck::save_config(&config.deck_path, &deck_config) {
            eprintln!("liquid-deck: cannot create {}: {error}", config.deck_path.display());
        }
    }
    let mut deck_state = deck::load_state(&config.state_path);
    deck_state.retain_for(&deck_config);

    if config.seed_sounds {
        if let Some(dir) = config.sounds_dir.as_deref() {
            let _ = sounds::install_defaults(dir);
        }
    }

    let (tx, _) = broadcast::channel::<Arc<String>>(8);
    let (events, _) = broadcast::channel::<Arc<String>>(32);
    let latest = Arc::new(RwLock::new(None));
    let history = Arc::new(Mutex::new(VecDeque::with_capacity(HISTORY_LEN)));
    let clients = Arc::new(AtomicUsize::new(0));
    let activity = Arc::new(AtomicU64::new(0));
    let wake = Arc::new(Notify::new());
    let ping_us = Arc::new(AtomicU32::new(PING_NONE));

    let executor = Arc::new(Executor::new(
        config.scripts_dir.clone(),
        config.sounds_dir.clone(),
        deck_config.settings.obs.clone(),
    ));

    let state = AppState {
        tx: tx.clone(),
        events,
        latest: latest.clone(),
        history: history.clone(),
        clients: clients.clone(),
        activity: activity.clone(),
        wake: wake.clone(),
        executor,
        config: Arc::new(config.clone()),
        gate: Arc::new(Mutex::new(ActionGate::new())),
        deck: Arc::new(RwLock::new(deck_config)),
        deck_state: Arc::new(RwLock::new(deck_state)),
    };

    metrics::spawn_sampler(
        tx,
        latest,
        history,
        clients.clone(),
        activity.clone(),
        wake,
        ping_us.clone(),
        metrics::SamplerConfig {
            sample_ms: config.sample_ms,
            idle_ms: config.idle_ms,
        },
    );
    metrics::spawn_ping(ping_us, clients, activity, config.ping_target.clone());

    let app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/health", get(health_handler))
        .route("/api/auth", get(auth_handler))
        .route("/api/metrics", get(metrics_handler))
        .route("/api/actions", get(actions_handler))
        .route("/api/action", post(action_handler))
        .route("/api/config", get(config_handler).put(config_put_handler))
        .route("/api/config/reset", post(config_reset_handler))
        .route("/api/state", post(state_handler))
        .route("/api/audio", get(audio_handler).post(audio_action_handler))
        .route("/api/sounds", get(sounds_handler))
        .route("/api/sound", get(sound_file_handler))
        .route("/api/obs/test", post(obs_test_handler))
        .fallback(static_handler)
        .layer(DefaultBodyLimit::max(128 * 1024))
        .layer(middleware::from_fn(security_headers))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|error| format!("cannot bind {bind}: {error}"))?;
    let local = listener
        .local_addr()
        .map_err(|error| format!("cannot read local address: {error}"))?;

    print_banner(&config, local).await;

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .map_err(|error| format!("server error: {error}"))?;

    println!("\n  Liquid Deck stopped.");
    Ok(())
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn health_handler(State(state): State<AppState>) -> Response {
    let latest = state.latest.read().await.clone();
    Json(json!({
        "ok": true,
        "version": VERSION,
        "host": latest.as_ref().map(|metrics| metrics.host.clone()).unwrap_or_default(),
        "os": latest.as_ref().map(|metrics| metrics.os.clone()).unwrap_or_default(),
        "uptime": latest.as_ref().map(|metrics| metrics.uptime).unwrap_or(0),
        "clients": state.clients.load(Ordering::Relaxed),
        "auth_required": state.config.token.is_some(),
        "scripts": state.config.scripts_dir.is_some(),
        "sample_ms": state.config.sample_ms,
    }))
    .into_response()
}

async fn auth_handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if state.authorized(&headers, query.get("token").map(String::as_str)) {
        Json(json!({ "ok": true })).into_response()
    } else {
        unauthorized()
    }
}

async fn actions_handler() -> Response {
    Json(json!({
        "ok": true,
        "actions": actions::ACTION_CATALOG,
    }))
    .into_response()
}

async fn metrics_handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if !state.authorized(&headers, query.get("token").map(String::as_str)) {
        return unauthorized();
    }

    // An HTTP poll is a viewer too: keep the sampler hot while the WS is down.
    state.activity.store(now_secs(), Ordering::Relaxed);
    state.wake.notify_one();

    let latest = state.latest.read().await.clone();
    match latest {
        Some(metrics) => {
            let mut value = serde_json::to_value(&metrics).unwrap_or_else(|_| json!({}));
            if let Value::Object(map) = &mut value {
                map.insert("type".into(), Value::String("metrics".into()));
            }
            Json(value).into_response()
        }
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "ok": false, "error": "warming_up" })),
        )
            .into_response(),
    }
}

async fn action_handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    Json(request): Json<ActionRequest>,
) -> Response {
    if !state.authorized(&headers, query.get("token").map(String::as_str)) {
        return unauthorized();
    }

    state.activity.store(now_secs(), Ordering::Relaxed);

    if !state.allow_action().await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "ok": false, "message": "Action rate limit exceeded. Slow down." })),
        )
            .into_response();
    }

    let outcome = state.executor.execute(request).await;
    Json(outcome).into_response()
}

// ---------------------------------------------------------------------------
// Deck configuration
// ---------------------------------------------------------------------------

async fn config_handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if !state.authorized(&headers, query.get("token").map(String::as_str)) {
        return unauthorized();
    }

    let config = state.deck.read().await.clone();
    let deck_state = state.deck_state.read().await.clone();
    Json(json!({
        "ok": true,
        "config": config,
        "state": deck_state,
        "sounds": sounds::list(state.config.sounds_dir.as_deref()),
        "sounds_enabled": state.config.sounds_dir.is_some(),
    }))
    .into_response()
}

async fn config_put_handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    Json(mut config): Json<DeckConfig>,
) -> Response {
    if !state.authorized(&headers, query.get("token").map(String::as_str)) {
        return unauthorized();
    }

    if let Err(error) = config.normalize() {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({ "ok": false, "message": error })),
        )
            .into_response();
    }

    if let Err(error) = deck::save_config(&state.config.deck_path, &config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "ok": false, "message": error })),
        )
            .into_response();
    }

    let mut state_payload = {
        let mut current = state.deck_state.write().await;
        current.retain_for(&config);
        current.clone()
    };
    if !config
        .boards
        .iter()
        .any(|board| board.id == state_payload.active_board)
    {
        state_payload.active_board = config.settings.default_board.clone();
    }
    let warning = deck::save_state(&state.config.state_path, &state_payload).err();

    state.executor.set_obs(config.settings.obs.clone());
    *state.deck.write().await = config.clone();
    *state.deck_state.write().await = state_payload.clone();

    publish(
        &state,
        json!({ "type": "config", "config": config, "state": state_payload }),
    );

    Json(json!({
        "ok": true,
        "config": config,
        "state": state_payload,
        "warning": warning,
        "sounds": sounds::list(state.config.sounds_dir.as_deref()),
    }))
    .into_response()
}

async fn config_reset_handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if !state.authorized(&headers, query.get("token").map(String::as_str)) {
        return unauthorized();
    }

    let mut config = DeckConfig::default();
    if let Err(error) = config.normalize() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "ok": false, "message": error })),
        )
            .into_response();
    }
    if let Err(error) = deck::save_config(&state.config.deck_path, &config) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "ok": false, "message": error })),
        )
            .into_response();
    }

    let fresh = DeckState {
        active_board: config.settings.default_board.clone(),
        ..DeckState::default()
    };
    let _ = deck::save_state(&state.config.state_path, &fresh);

    state.executor.set_obs(config.settings.obs.clone());
    *state.deck.write().await = config.clone();
    *state.deck_state.write().await = fresh.clone();

    publish(
        &state,
        json!({ "type": "config", "config": config, "state": fresh }),
    );

    Json(json!({
        "ok": true,
        "config": config,
        "state": fresh,
        "sounds": sounds::list(state.config.sounds_dir.as_deref()),
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
struct StateUpdate {
    #[serde(default)]
    board: Option<String>,
    #[serde(default)]
    button: Option<String>,
    #[serde(default)]
    active: Option<bool>,
    #[serde(default)]
    active_board: Option<String>,
}

async fn state_handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    Json(update): Json<StateUpdate>,
) -> Response {
    if !state.authorized(&headers, query.get("token").map(String::as_str)) {
        return unauthorized();
    }

    let config = state.deck.read().await.clone();
    let mut current = state.deck_state.write().await;

    if let Some(board) = update.active_board {
        if !config.boards.iter().any(|candidate| candidate.id == board) {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "ok": false, "message": "Unknown board." })),
            )
                .into_response();
        }
        current.active_board = board;
        current.updated_at = now_secs();
    } else {
        let (Some(board), Some(button), Some(active)) =
            (update.board, update.button, update.active)
        else {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "ok": false,
                    "message": "Send active_board, or board + button + active.",
                })),
            )
                .into_response();
        };

        let known = config
            .boards
            .iter()
            .find(|candidate| candidate.id == board)
            .is_some_and(|candidate| {
                candidate.buttons.iter().any(|entry| entry.id == button)
            });
        if !known {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "ok": false, "message": "Unknown button." })),
            )
                .into_response();
        }
        current.set(&board, &button, active);
    }

    let payload = current.clone();
    drop(current);
    let warning = deck::save_state(&state.config.state_path, &payload).err();
    publish(&state, json!({ "type": "deck_state", "state": payload }));

    Json(json!({ "ok": true, "state": payload, "warning": warning })).into_response()
}

// ---------------------------------------------------------------------------
// Audio mixer, sounds and OBS
// ---------------------------------------------------------------------------

async fn audio_handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if !state.authorized(&headers, query.get("token").map(String::as_str)) {
        return unauthorized();
    }
    Json(audio::snapshot().await).into_response()
}

#[derive(Debug, Deserialize)]
struct AudioRequest {
    #[serde(default)]
    op: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    value: Option<u8>,
}

async fn audio_action_handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    Json(request): Json<AudioRequest>,
) -> Response {
    if !state.authorized(&headers, query.get("token").map(String::as_str)) {
        return unauthorized();
    }

    if !state.allow_action().await {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "ok": false, "message": "Action rate limit exceeded." })),
        )
            .into_response();
    }

    state.activity.store(now_secs(), Ordering::Relaxed);
    match audio::apply(&request.op, &request.id, request.value).await {
        Ok(message) => Json(json!({ "ok": true, "message": message })).into_response(),
        Err(message) => Json(json!({ "ok": false, "message": message })).into_response(),
    }
}

async fn sounds_handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if !state.authorized(&headers, query.get("token").map(String::as_str)) {
        return unauthorized();
    }
    Json(json!({
        "ok": true,
        "enabled": state.config.sounds_dir.is_some(),
        "sounds": sounds::list(state.config.sounds_dir.as_deref()),
    }))
    .into_response()
}

async fn sound_file_handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    if !state.authorized(&headers, query.get("token").map(String::as_str)) {
        return unauthorized();
    }

    let name = query.get("name").cloned().unwrap_or_default();
    let path = match sounds::resolve(state.config.sounds_dir.as_deref(), &name) {
        Ok(path) => path,
        Err(error) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({ "ok": false, "message": error })),
            )
                .into_response()
        }
    };
    let data = match tokio::fs::read(&path).await {
        Ok(data) => data,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "ok": false, "message": error.to_string() })),
            )
                .into_response()
        }
    };

    let length = data.len();
    let mut response = Response::new(Body::from(data));
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(sounds::media_type(&path)),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert(header::ACCEPT_RANGES, HeaderValue::from_static("none"));
    if let Ok(value) = HeaderValue::from_str(&length.to_string()) {
        headers.insert(header::CONTENT_LENGTH, value);
    }
    response
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ObsTestRequest {
    url: Option<String>,
    password: Option<String>,
}

async fn obs_test_handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    Json(request): Json<ObsTestRequest>,
) -> Response {
    if !state.authorized(&headers, query.get("token").map(String::as_str)) {
        return unauthorized();
    }

    let saved = state.deck.read().await.settings.obs.clone();
    let settings = deck::ObsSettings {
        url: request.url.unwrap_or(saved.url).trim().to_string(),
        password: request.password.unwrap_or(saved.password),
    };

    if let Err(error) = deck::validate_ws_url(&settings.url) {
        return Json(json!({ "ok": false, "message": error })).into_response();
    }

    match obs::execute(&settings, "test", "").await {
        Ok(message) => Json(json!({ "ok": true, "message": message })).into_response(),
        Err(message) => Json(json!({ "ok": false, "message": message })).into_response(),
    }
}

fn publish(state: &AppState, value: Value) {
    let _ = state.events.send(Arc::new(value.to_string()));
}

// ---------------------------------------------------------------------------
// WebSocket
// ---------------------------------------------------------------------------

async fn ws_handler(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if !state.authorized(&headers, query.get("token").map(String::as_str)) {
        return unauthorized();
    }

    ws.max_message_size(64 * 1024)
        .max_frame_size(16 * 1024)
        .on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    state.clients.fetch_add(1, Ordering::Relaxed);
    state.activity.store(now_secs(), Ordering::Relaxed);
    state.wake.notify_one();
    let _guard = ClientGuard(state.clients.clone());

    let mut receiver = state.tx.subscribe();
    let mut events = state.events.subscribe();
    let latest = state.latest.read().await.clone();
    let history: Vec<HistoryPoint> = state.history.lock().await.iter().cloned().collect();
    let deck_config = state.deck.read().await.clone();
    let deck_state = state.deck_state.read().await.clone();

    let hello = json!({
        "type": "hello",
        "version": VERSION,
        "host": latest.as_ref().map(|metrics| metrics.host.clone()).unwrap_or_default(),
        "os": latest.as_ref().map(|metrics| metrics.os.clone()).unwrap_or_default(),
        "interval_ms": state.config.sample_ms,
        "history": history,
        "metrics": latest,
        "actions": actions::ACTION_CATALOG,
        "auth_required": state.config.token.is_some(),
        "scripts": state.config.scripts_dir.is_some(),
        "sounds_enabled": state.config.sounds_dir.is_some(),
        "sounds": sounds::list(state.config.sounds_dir.as_deref()),
        "config": deck_config,
        "state": deck_state,
    });

    if socket
        .send(Message::Text(hello.to_string().into()))
        .await
        .is_err()
    {
        return;
    }

    let (out_tx, mut out_rx) = mpsc::channel::<String>(16);
    let mut heartbeat = tokio::time::interval(Duration::from_secs(25));
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await; // consume the immediate first tick

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        handle_client_text(&state, &text, &out_tx).await;
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) | Some(Ok(Message::Binary(_))) => {}
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(_)) => break,
                }
            }
            update = receiver.recv() => {
                match update {
                    Ok(payload) => {
                        if socket.send(Message::Text(payload.as_str().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            Some(outgoing) = out_rx.recv() => {
                if socket.send(Message::Text(outgoing.into())).await.is_err() {
                    break;
                }
            }
            event = events.recv() => {
                match event {
                    Ok(payload) => {
                        if socket.send(Message::Text(payload.as_str().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
            _ = heartbeat.tick() => {
                if socket.send(Message::Ping(Default::default())).await.is_err() {
                    break;
                }
            }
        }
    }
}

async fn handle_client_text(state: &AppState, text: &str, out: &mpsc::Sender<String>) {
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => {
            send_json(
                out,
                json!({ "type": "error", "message": "Invalid JSON payload." }),
            )
            .await;
            return;
        }
    };

    match value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default()
    {
        "ping" => {
            let t = value.get("t").cloned().unwrap_or(Value::Null);
            send_json(out, json!({ "type": "pong", "t": t })).await;
        }
        "action" => {
            let request_id = value.get("request_id").cloned();
            let mut object = match value {
                Value::Object(map) => map,
                _ => {
                    send_json(
                        out,
                        action_error(request_id, "Action payload must be an object."),
                    )
                    .await;
                    return;
                }
            };
            object.remove("type");
            object.remove("request_id");

            let request: ActionRequest = match serde_json::from_value(Value::Object(object)) {
                Ok(request) => request,
                Err(error) => {
                    send_json(
                        out,
                        action_error(request_id, format!("Invalid action: {error}")),
                    )
                    .await;
                    return;
                }
            };

            if !state.allow_action().await {
                send_json(
                    out,
                    action_error(request_id, "Action rate limit exceeded. Slow down."),
                )
                .await;
                return;
            }

            state.activity.store(now_secs(), Ordering::Relaxed);
            let executor = state.executor.clone();
            let out = out.clone();

            // Run the action off the socket loop so metrics keep flowing even
            // when a script takes a few seconds.
            tokio::spawn(async move {
                let outcome = executor.execute(request).await;
                let mut payload = serde_json::to_value(&outcome)
                    .unwrap_or_else(|_| json!({ "ok": false, "message": "Serialization error." }));
                if let Value::Object(map) = &mut payload {
                    map.insert("type".into(), Value::String("action_result".into()));
                    if let Some(id) = request_id {
                        map.insert("request_id".into(), id);
                    }
                }
                let _ = out.send(payload.to_string()).await;
            });
        }
        other => {
            send_json(
                out,
                json!({
                    "type": "error",
                    "message": format!("Unknown message type: {other}"),
                }),
            )
            .await;
        }
    }
}

fn action_error(request_id: Option<Value>, message: impl Into<String>) -> Value {
    let mut value = json!({
        "type": "action_result",
        "ok": false,
        "message": message.into(),
    });
    if let Some(id) = request_id {
        if let Value::Object(map) = &mut value {
            map.insert("request_id".into(), id);
        }
    }
    value
}

async fn send_json(out: &mpsc::Sender<String>, value: Value) {
    let _ = out.send(value.to_string()).await;
}

// ---------------------------------------------------------------------------
// Static assets
// ---------------------------------------------------------------------------

async fn static_handler(method: Method, uri: Uri, headers: HeaderMap) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            [(header::ALLOW, "GET, HEAD")],
            "Method not allowed",
        )
            .into_response();
    }

    let mut path = uri.path().trim_start_matches('/');
    if path.is_empty() {
        path = "index.html";
    }

    if path.contains("..") || path.contains('\0') {
        return not_found();
    }

    if path.starts_with("api/") {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({ "ok": false, "error": "not_found" })),
        )
            .into_response();
    }

    let (asset_path, forced_type) = if path == "favicon.ico" {
        ("icon.svg", Some("image/svg+xml"))
    } else {
        (path, None)
    };

    let Some(file) = Assets::get(asset_path) else {
        return not_found();
    };

    let data = file.data;
    let tag = etag(&data);
    let cache = if asset_path == "index.html" {
        "no-cache"
    } else {
        "public, max-age=3600"
    };

    if let Some(if_none_match) = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
    {
        if if_none_match.split(',').any(|value| value.trim() == tag) {
            let mut response = StatusCode::NOT_MODIFIED.into_response();
            let out = response.headers_mut();
            if let Ok(value) = HeaderValue::from_str(&tag) {
                out.insert(header::ETAG, value);
            }
            out.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
            return response;
        }
    }

    let mime = forced_type.unwrap_or_else(|| content_type(asset_path));
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(data.into_owned())
    };
    let mut response = Response::new(body);
    let out = response.headers_mut();
    out.insert(header::CONTENT_TYPE, HeaderValue::from_static(mime));
    out.insert(header::CACHE_CONTROL, HeaderValue::from_static(cache));
    if let Ok(value) = HeaderValue::from_str(&tag) {
        out.insert(header::ETAG, value);
    }
    response
}

async fn security_headers(request: Request, next: Next) -> Response {
    let is_api = request.uri().path().starts_with("/api/");
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    headers.insert("x-frame-options", HeaderValue::from_static("DENY"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    headers.insert(
        "content-security-policy",
        HeaderValue::from_static(
            "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
             img-src 'self' data:; media-src 'self' blob:; connect-src 'self' ws: wss:; \
             font-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'",
        ),
    );
    if is_api {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }

    response
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer realm=\"liquid-deck\"")],
        Json(json!({ "ok": false, "error": "unauthorized" })),
    )
        .into_response()
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "Not found",
    )
        .into_response()
}

fn content_type(path: &str) -> &'static str {
    let extension = path
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json; charset=utf-8",
        "webmanifest" => "application/manifest+json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/x-icon",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// FNV-1a content hash, used as a strong-enough ETag for embedded assets.
fn etag(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("\"{hash:016x}\"")
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

fn parse_config() -> Result<Config, String> {
    let mut bind_raw = env::var("LIQUID_DECK_BIND").ok();
    let mut port: u16 = env::var("LIQUID_DECK_PORT")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(8787);
    let mut token = env::var("LIQUID_DECK_TOKEN").ok();
    let mut scripts = env::var("LIQUID_DECK_SCRIPTS").ok();
    let mut data = env::var("LIQUID_DECK_DATA").ok();
    let mut config_path = env::var("LIQUID_DECK_CONFIG").ok();
    let mut sounds = env::var("LIQUID_DECK_SOUNDS").ok();
    let mut no_sounds = false;
    let mut ping_target =
        env::var("LIQUID_DECK_PING_TARGET").unwrap_or_else(|_| "1.1.1.1:443".to_string());
    let sample_ms: u64 = env::var("LIQUID_DECK_INTERVAL_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1_000);
    let idle_ms: u64 = env::var("LIQUID_DECK_IDLE_MS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(5_000);

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("liquid-deck {VERSION}");
                std::process::exit(0);
            }
            "--port" => {
                let value = args.next().ok_or("--port requires a value")?;
                port = value
                    .parse()
                    .map_err(|_| format!("invalid port: {value}"))?;
            }
            "--bind" | "--host" => {
                bind_raw = Some(args.next().ok_or("--bind requires an address")?);
            }
            "--lan" => bind_raw = Some("0.0.0.0".to_string()),
            "--local" => bind_raw = Some("127.0.0.1".to_string()),
            "--token" => {
                token = Some(args.next().ok_or("--token requires a value")?);
            }
            "--scripts" => {
                scripts = Some(args.next().ok_or("--scripts requires a directory")?);
            }
            "--data" => {
                data = Some(args.next().ok_or("--data requires a directory")?);
            }
            "--config" => {
                config_path = Some(args.next().ok_or("--config requires a file")?);
            }
            "--sounds" => {
                sounds = Some(args.next().ok_or("--sounds requires a directory")?);
                no_sounds = false;
            }
            "--no-sounds" => {
                no_sounds = true;
                sounds = None;
            }
            "--ping" => {
                ping_target = args.next().ok_or("--ping requires host:port")?;
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }

    let bind_raw = bind_raw.unwrap_or_else(|| "127.0.0.1".to_string());
    let bind = if let Ok(address) = bind_raw.parse::<SocketAddr>() {
        address
    } else if let Ok(ip) = bind_raw.parse::<IpAddr>() {
        SocketAddr::new(ip, port)
    } else {
        return Err(format!("invalid bind address: {bind_raw}"));
    };

    let token = token
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let scripts_dir = scripts
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);

    let data_dir = data
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(deck::default_data_dir);

    let deck_path = config_path
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("boards.json"));

    let state_path = deck_path
        .parent()
        .map(|parent| parent.join("state.json"))
        .unwrap_or_else(|| data_dir.join("state.json"));

    let explicit_sounds = sounds
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let seed_sounds = !no_sounds && explicit_sounds.is_none();
    let sounds_dir = if no_sounds {
        None
    } else {
        Some(explicit_sounds.unwrap_or_else(|| data_dir.join("sounds")))
    };

    let ping_target = if ping_target.contains(':') {
        ping_target
    } else {
        format!("{ping_target}:443")
    };

    Ok(Config {
        bind,
        token,
        scripts_dir,
        sounds_dir,
        seed_sounds,
        deck_path,
        state_path,
        ping_target,
        sample_ms: sample_ms.clamp(250, 10_000),
        idle_ms: idle_ms.clamp(1_000, 60_000),
    })
}

fn print_help() {
    println!(
        "\
Liquid Deck {VERSION} — Stream Deck & live hardware dashboard

USAGE:
  liquid-deck [OPTIONS]

OPTIONS:
  --bind <ADDR>       Bind address, IP or IP:PORT (default 127.0.0.1:8787)
  --port <PORT>       Port used when --bind has no port (default 8787)
  --lan               Shortcut for --bind 0.0.0.0 (reachable from your phone)
  --local             Shortcut for --bind 127.0.0.1
  --token <TOKEN>     Require this token for metrics, WebSocket and actions
  --scripts <DIR>     Enable script actions from this directory
  --config <FILE>     Board layout file (default <data>/boards.json)
  --data <DIR>        Data directory for config, state and sounds
  --sounds <DIR>      Soundboard directory (default <data>/sounds)
  --no-sounds         Disable the soundboard
  --ping <HOST:PORT>  TCP ping target (default 1.1.1.1:443)
  -h, --help          Show this help
  -V, --version       Show the version

ENVIRONMENT:
  LIQUID_DECK_BIND, LIQUID_DECK_PORT, LIQUID_DECK_TOKEN,
  LIQUID_DECK_SCRIPTS, LIQUID_DECK_PING_TARGET,
  LIQUID_DECK_CONFIG, LIQUID_DECK_DATA, LIQUID_DECK_SOUNDS,
  LIQUID_DECK_INTERVAL_MS, LIQUID_DECK_IDLE_MS,
  LIQUID_DECK_LAUNCH_<TARGET>, LIQUID_DECK_GAME_MODE_ON,
  LIQUID_DECK_GAME_MODE_OFF

EXAMPLES:
  liquid-deck --lan --token my-secret
  liquid-deck --scripts ./scripts --sounds ~/Music/clips
  liquid-deck --config ./boards.json
  curl -X POST localhost:8787/api/action \\
    -H 'content-type: application/json' \\
    -d '{{\"action\":\"media\",\"command\":\"play_pause\"}}'
"
    );
}

async fn print_banner(config: &Config, local: SocketAddr) {
    let port = local.port();
    println!();
    println!("  Liquid Deck v{VERSION}");
    if local.ip().is_loopback() {
        println!("  Local    http://127.0.0.1:{port}");
    } else {
        println!("  Listen   http://{local}");
        if let Some(lan) = lan_hint(port).await {
            println!("  Network  {lan}");
        }
    }
    println!(
        "  Auth     {}",
        if config.token.is_some() {
            "token required"
        } else {
            "open (no token)"
        }
    );
    println!(
        "  Scripts  {}",
        config
            .scripts_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "disabled".to_string())
    );
    println!(
        "  Sounds   {}",
        config
            .sounds_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "disabled".to_string())
    );
    println!("  Config   {}", config.deck_path.display());
    println!(
        "  Sampling {} ms active / {} ms idle · ping {}",
        config.sample_ms, config.idle_ms, config.ping_target
    );

    if !local.ip().is_loopback() && config.token.is_none() {
        eprintln!();
        eprintln!("  ⚠  This deck is reachable from the network without a token.");
        eprintln!("     Anyone who can reach port {port} can trigger actions or edit boards.");
        eprintln!("     Restart with --token <secret> before using it on a LAN.");
    }
    println!();
}

async fn lan_hint(port: u16) -> Option<String> {
    let socket = tokio::net::UdpSocket::bind("0.0.0.0:0").await.ok()?;
    // UDP connect only selects a route; it does not send any packet.
    socket.connect("8.8.8.8:80").await.ok()?;
    let ip = socket.local_addr().ok()?.ip();
    if ip.is_unspecified() || ip.is_loopback() {
        None
    } else {
        Some(format!("http://{ip}:{port}"))
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        difference |= a ^ b;
    }
    difference == 0
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_comparison_is_correct() {
        assert!(constant_time_eq(b"secret", b"secret"));
        assert!(!constant_time_eq(b"secret", b"secrez"));
        assert!(!constant_time_eq(b"secret", b"secret-longer"));
    }

    #[test]
    fn content_types_are_mapped() {
        assert_eq!(content_type("app.js"), "text/javascript; charset=utf-8");
        assert_eq!(content_type("style.css"), "text/css; charset=utf-8");
        assert_eq!(content_type("icon.svg"), "image/svg+xml");
        assert_eq!(content_type("unknown.bin"), "application/octet-stream");
    }

    #[test]
    fn etags_change_with_content() {
        assert_ne!(etag(b"a"), etag(b"b"));
        assert_eq!(etag(b"same"), etag(b"same"));
    }
}
