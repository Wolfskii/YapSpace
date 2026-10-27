//! `GET /v1/realtime`: the WebSocket upgrade (docs/architecture/realtime.md §G.2, §G.3). Not in the REST document,
//! because it is a stream, not a request/response pair.

use axum::{
    extract::{State, ws::WebSocketUpgrade},
    http::{HeaderMap, StatusCode, header::ORIGIN},
    response::{IntoResponse, Response},
};

use crate::{app::AppState, client_ip::ClientIp, config::Env, realtime::slots::Refused};

/// Browser extensions connect from these origins. Ordinary web pages do not.
const EXTENSION_ORIGINS: &[&str] = &[
    "chrome-extension://",
    "moz-extension://",
    "safari-web-extension://",
];

/// Origin rules: extensions always pass. A missing Origin passes only in dev, so tests and tools work.
fn origin_allowed(headers: &HeaderMap, state: &AppState) -> bool {
    match headers.get(ORIGIN).and_then(|value| value.to_str().ok()) {
        None => state.config.env == Env::Dev,
        Some(origin) => {
            EXTENSION_ORIGINS.iter().any(|prefix| origin.starts_with(prefix))
                || state
                    .config
                    .allowed_ws_origins
                    .iter()
                    .any(|allowed| allowed == origin)
        }
    }
}

pub async fn connect(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    if !origin_allowed(&headers, &state) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let slot = match state.ws.acquire_ip(ip) {
        Ok(slot) => slot,
        Err(Refused::PerIp) => return StatusCode::TOO_MANY_REQUESTS.into_response(),
        Err(Refused::Total) => return StatusCode::SERVICE_UNAVAILABLE.into_response(),
    };
    let connection_state = state.clone();
    ws.read_buffer_size(4096)
        .write_buffer_size(8192)
        .max_write_buffer_size(65536)
        .max_message_size(4096)
        .max_frame_size(4096)
        .on_upgrade(move |socket| crate::realtime::connection::run(socket, connection_state, slot))
}
