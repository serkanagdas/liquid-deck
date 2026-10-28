//! Minimal obs-websocket v5 client (OBS Studio 28+).
//!
//! One short-lived WebSocket session is opened per action: identify, send one
//! request, read the response, close. This keeps the deck stateless and means
//! OBS can be restarted without leaving the server with a dead socket.
//!
//! Only `ws://` is supported because the release build intentionally ships no
//! TLS stack; point it at a local OBS instance or a TLS-terminating proxy.

use std::time::Duration;

use base64::{engine::general_purpose::STANDARD, Engine};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::{net::TcpStream, time::timeout};
use tokio_tungstenite::{
    connect_async,
    tungstenite::Message,
    MaybeTlsStream, WebSocketStream,
};

use crate::deck::{validate_ws_url, ObsSettings, OBS_OPS};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const TIMEOUT: Duration = Duration::from_secs(8);
const REQUEST_ID: &str = "liquid-deck-1";

pub async fn execute(settings: &ObsSettings, op: &str, target: &str) -> Result<String, String> {
    let op = op.trim().to_ascii_lowercase();
    if !OBS_OPS.contains(&op.as_str()) {
        return Err(format!("Unknown OBS operation: {op}"));
    }
    validate_ws_url(&settings.url)?;

    match timeout(TIMEOUT, session(settings, &op, target.trim())).await {
        Ok(result) => result,
        Err(_) => Err("OBS WebSocket timed out. Is OBS running and obs-websocket enabled?".to_string()),
    }
}

async fn session(settings: &ObsSettings, op: &str, target: &str) -> Result<String, String> {
    let (mut socket, _) = connect_async(settings.url.trim())
        .await
        .map_err(|error| {
            format!(
                "OBS connect failed: {error}. Check Tools → WebSocket Server Settings in OBS."
            )
        })?;

    let hello = next_json(&mut socket).await?;
    if hello.get("op").and_then(Value::as_u64) != Some(0) {
        return Err("OBS did not send a Hello message.".to_string());
    }
    let details = hello.get("d").cloned().unwrap_or_else(|| json!({}));
    let rpc_version = details
        .get("rpcVersion")
        .and_then(Value::as_u64)
        .unwrap_or(1);

    let mut identify = json!({
        "rpcVersion": rpc_version,
        "eventSubscriptions": 0,
    });

    if let Some(auth) = details.get("authentication") {
        let challenge = auth
            .get("challenge")
            .and_then(Value::as_str)
            .ok_or("OBS auth challenge missing.")?;
        let salt = auth
            .get("salt")
            .and_then(Value::as_str)
            .ok_or("OBS auth salt missing.")?;
        if settings.password.is_empty() {
            return Err(
                "OBS requires a password. Add it under Integrations → OBS.".to_string(),
            );
        }
        identify["authentication"] = Value::String(auth_string(
            &settings.password,
            salt,
            challenge,
        ));
    }

    send_json(&mut socket, json!({ "op": 1, "d": identify })).await?;

    let identified = next_json(&mut socket).await?;
    match identified.get("op").and_then(Value::as_u64) {
        Some(2) => {}
        _ => {
            return Err(
                "OBS rejected the WebSocket session (check the password).".to_string(),
            )
        }
    }

    let (request_type, request_data, label) = request_for(op, target)?;
    send_json(
        &mut socket,
        json!({
            "op": 6,
            "d": {
                "requestType": request_type,
                "requestId": REQUEST_ID,
                "requestData": request_data,
            }
        }),
    )
    .await?;

    loop {
        let message = next_json(&mut socket).await?;
        match message.get("op").and_then(Value::as_u64) {
            Some(7) => {
                if message
                    .pointer("/d/requestId")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id != REQUEST_ID)
                {
                    continue;
                }
                let status = message.pointer("/d/requestStatus").cloned().unwrap_or_default();
                let result = status
                    .get("result")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if !result {
                    let code = status.get("code").and_then(Value::as_u64).unwrap_or(0);
                    let comment = status
                        .get("comment")
                        .and_then(Value::as_str)
                        .unwrap_or("OBS rejected the request");
                    return Err(format!("{comment} (code {code})"));
                }
                let _ = socket.close(None).await;

                if op == "test" {
                    let version = message
                        .pointer("/d/responseData/obsWebSocketVersion")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown");
                    let obs = message
                        .pointer("/d/responseData/obsVersion")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    let suffix = if obs.is_empty() {
                        String::new()
                    } else {
                        format!(" · OBS {obs}")
                    };
                    return Ok(format!("Connected · obs-websocket {version}{suffix}"));
                }

                return Ok(label);
            }
            Some(9) => return Err("OBS returned an unexpected batch response.".to_string()),
            Some(0) => return Err("OBS re-sent Hello unexpectedly.".to_string()),
            _ => continue,
        }
    }
}

fn request_for(op: &str, target: &str) -> Result<(&'static str, Value, String), String> {
    let need_target = || -> Result<(), String> {
        if target.is_empty() {
            Err("This OBS action needs a scene or source name.".to_string())
        } else {
            Ok(())
        }
    };

    Ok(match op {
        "scene" => {
            need_target()?;
            (
                "SetCurrentProgramScene",
                json!({ "sceneName": target }),
                format!("OBS scene → {target}"),
            )
        }
        "record_start" => ("StartRecord", json!({}), "OBS recording started".to_string()),
        "record_stop" => ("StopRecord", json!({}), "OBS recording stopped".to_string()),
        "record_toggle" => ("ToggleRecord", json!({}), "OBS recording toggled".to_string()),
        "stream_start" => ("StartStream", json!({}), "OBS stream started".to_string()),
        "stream_stop" => ("StopStream", json!({}), "OBS stream stopped".to_string()),
        "stream_toggle" => ("ToggleStream", json!({}), "OBS stream toggled".to_string()),
        "mute_toggle" => {
            need_target()?;
            (
                "ToggleInputMute",
                json!({ "inputName": target }),
                format!("OBS mute toggled: {target}"),
            )
        }
        "mute_on" => {
            need_target()?;
            (
                "SetInputMute",
                json!({ "inputName": target, "inputMuted": true }),
                format!("OBS muted: {target}"),
            )
        }
        "mute_off" => {
            need_target()?;
            (
                "SetInputMute",
                json!({ "inputName": target, "inputMuted": false }),
                format!("OBS unmuted: {target}"),
            )
        }
        "replay_start" => (
            "StartReplayBuffer",
            json!({}),
            "OBS replay buffer started".to_string(),
        ),
        "replay_stop" => (
            "StopReplayBuffer",
            json!({}),
            "OBS replay buffer stopped".to_string(),
        ),
        "replay_save" => (
            "SaveReplayBuffer",
            json!({}),
            "OBS replay saved".to_string(),
        ),
        "scene_collection" => {
            need_target()?;
            (
                "SetCurrentSceneCollection",
                json!({ "sceneCollectionName": target }),
                format!("OBS collection → {target}"),
            )
        }
        "test" => ("GetVersion", json!({}), "OBS connected".to_string()),
        other => return Err(format!("Unknown OBS operation: {other}")),
    })
}

async fn next_json(socket: &mut Socket) -> Result<Value, String> {
    loop {
        let message = socket
            .next()
            .await
            .ok_or_else(|| "OBS closed the WebSocket.".to_string())?
            .map_err(|error| format!("OBS WebSocket error: {error}"))?;

        match message {
            Message::Text(text) => {
                return serde_json::from_str(text.as_str())
                    .map_err(|error| format!("Invalid OBS payload: {error}"));
            }
            Message::Binary(bytes) => {
                return serde_json::from_slice(&bytes)
                    .map_err(|error| format!("Invalid OBS payload: {error}"));
            }
            Message::Close(_) => return Err("OBS closed the WebSocket.".to_string()),
            _ => continue,
        }
    }
}

async fn send_json(socket: &mut Socket, value: Value) -> Result<(), String> {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .map_err(|error| format!("OBS send failed: {error}"))
}

/// obs-websocket v5 authentication:
/// `base64(sha256(base64(sha256(password + salt)) + challenge))`.
pub fn auth_string(password: &str, salt: &str, challenge: &str) -> String {
    let secret = sha256_base64(&format!("{password}{salt}"));
    sha256_base64(&format!("{secret}{challenge}"))
}

fn sha256_base64(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    STANDARD.encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_matches_the_protocol_formula() {
        // Pre-computed with an independent SHA-256 implementation.
        assert_eq!(
            auth_string("supersecret", "salt123", "challenge456"),
            "V8pVriFPEtnaK7wzQPlqOgkXegTAwSevsIeJLiFx/Nw="
        );
    }

    #[test]
    fn request_mapping_is_restricted() {
        let (kind, data, _) = request_for("scene", "Main").unwrap();
        assert_eq!(kind, "SetCurrentProgramScene");
        assert_eq!(data["sceneName"], "Main");
        assert!(request_for("scene", "").is_err());
        assert!(request_for("format_disk", "").is_err());
    }

    #[test]
    fn auth_is_deterministic() {
        let a = auth_string("pw", "salt", "challenge");
        let b = auth_string("pw", "salt", "challenge");
        assert_eq!(a, b);
        assert_ne!(a, auth_string("other", "salt", "challenge"));
    }
}
