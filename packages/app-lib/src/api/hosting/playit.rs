use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;

const API_BASE: &str = "https://api.playit.gg";
// Reported to playit's claim API so it doesn't reject us as an outdated
// agent; keep this in sync with `PLAYIT_AGENT_TAG` in agent.rs
const AGENT_VERSION: &str = "playit 1.0.10";

#[derive(serde::Deserialize)]
#[serde(tag = "status", content = "data")]
enum ApiResult<S> {
    #[serde(rename = "success")]
    Success(S),
    #[serde(rename = "fail")]
    Fail(serde_json::Value),
    #[serde(rename = "error")]
    Error(serde_json::Value),
}

async fn call<Req: Serialize, Res: DeserializeOwned>(
    path: &str,
    secret: Option<&str>,
    req: &Req,
) -> crate::Result<Res> {
    let client = reqwest::Client::new();
    let mut builder = client.post(format!("{API_BASE}{path}")).json(req);

    if let Some(secret) = secret {
        builder = builder.header(
            reqwest::header::AUTHORIZATION,
            format!("Agent-Key {}", secret.trim()),
        );
    }

    let text = builder.send().await?.text().await?;
    let parsed: ApiResult<Res> = serde_json::from_str(&text).map_err(|e| {
        crate::ErrorKind::OtherError(format!(
            "Failed to parse playit response ({path}): {e}: {text}"
        ))
    })?;

    match parsed {
        ApiResult::Success(value) => Ok(value),
        ApiResult::Fail(value) => Err(crate::ErrorKind::OtherError(format!(
            "playit request failed ({path}): {value}"
        ))
        .into()),
        ApiResult::Error(value) => Err(crate::ErrorKind::OtherError(format!(
            "playit error ({path}): {value}"
        ))
        .into()),
    }
}

#[derive(serde::Serialize, Clone)]
pub struct ClaimInfo {
    pub code: String,
    pub url: String,
}

#[derive(serde::Serialize, Clone)]
pub struct ClaimPoll {
    pub status: String,
    pub secret: Option<String>,
}

pub const AGENT_NAME: &str = "Kael Launcher";

pub fn begin_claim() -> ClaimInfo {
    let code: String = (0..5)
        .map(|_| format!("{:02x}", rand::random::<u8>()))
        .collect();
    let url = format!("https://playit.gg/claim/{code}");
    ClaimInfo { code, url }
}

pub async fn poll_claim(
    code: &str,
    agent_type: &str,
) -> crate::Result<ClaimPoll> {
    let setup: String = call(
        "/claim/setup",
        None,
        &json!({
            "code": code,
            "agent_type": agent_type,
            "version": AGENT_VERSION,
        }),
    )
    .await?;

    match setup.as_str() {
        "UserAccepted" => {
            #[derive(serde::Deserialize)]
            struct AgentSecretKey {
                secret_key: String,
            }

            let secret: AgentSecretKey =
                call("/claim/exchange", None, &json!({ "code": code })).await?;

            Ok(ClaimPoll {
                status: "accepted".to_string(),
                secret: Some(secret.secret_key),
            })
        }
        "UserRejected" => Ok(ClaimPoll {
            status: "rejected".to_string(),
            secret: None,
        }),
        "WaitingForUser" => Ok(ClaimPoll {
            status: "waiting_user".to_string(),
            secret: None,
        }),
        _ => Ok(ClaimPoll {
            status: "waiting_visit".to_string(),
            secret: None,
        }),
    }
}

pub async fn login_guest(secret: &str) -> crate::Result<String> {
    #[derive(serde::Deserialize)]
    struct WebSession {
        session_key: String,
    }

    let session: WebSession =
        call("/login/guest", Some(secret), &json!({})).await?;

    Ok(format!(
        "https://playit.gg/login/guest-account/{}",
        session.session_key
    ))
}

// The `/tunnels/create` REST call requires a real, already-registered agent
// id: playit ties tunnels to an agent identity, not to a bare secret key.
// The agent record can exist before its daemon ever connects (the claim
// portal creates it), but playit rejects tunnel creation with errors like
// AgentVersionTooOld until the daemon has connected and reported its
// version - so this retries through that window instead of failing on the
// first attempt
pub async fn create_tunnel(
    secret: &str,
    name: &str,
    agent_id: &str,
    local_port: u16,
) -> crate::Result<String> {
    #[derive(serde::Deserialize)]
    struct ObjectId {
        id: String,
    }

    let req = json!({
        "name": name,
        "tunnel_type": "minecraft-java",
        "port_type": "tcp",
        "port_count": 1,
        "origin": {
            "type": "agent",
            "data": {
                "agent_id": agent_id,
                "local_ip": "127.0.0.1",
                "local_port": local_port,
            },
        },
        "enabled": true,
        "alloc": null,
        "firewall_id": null,
        "proxy_protocol": null,
    });

    let mut last_error = None;
    for attempt in 0..30 {
        match call::<_, ObjectId>("/tunnels/create", Some(secret), &req).await {
            Ok(object) => return Ok(object.id),
            Err(e) => {
                let message = e.to_string();
                let agent_still_registering = message
                    .contains("AgentVersionTooOld")
                    || message.contains("InvalidAgentId")
                    || message.contains("AgentNotFound");

                if !agent_still_registering {
                    return Err(e);
                }

                if attempt == 0 {
                    tracing::info!(
                        "playit agent still registering, retrying tunnel creation: {message}"
                    );
                }
                last_error = Some(e);
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| {
        crate::ErrorKind::OtherError(
            "Timed out creating the playit tunnel".to_string(),
        )
        .into()
    }))
}

/// Checks whether the agent behind this secret key still exists on playit's
/// side. Returns `Ok(false)` only when playit definitively rejects the key
/// (e.g. the user deleted the agent from the playit.gg web portal); network
/// or other errors propagate so a flaky connection can't be mistaken for a
/// deleted agent.
pub async fn agent_alive(secret: &str) -> crate::Result<bool> {
    let result: crate::Result<serde_json::Value> =
        call("/agents/rundata", Some(secret), &json!({})).await;

    match result {
        Ok(_) => Ok(true),
        Err(e) => {
            let message = e.to_string();
            if message.contains("InvalidAgentKey")
                || message.contains("NoLongerValid")
                || message.contains("AccountDoesNotExist")
            {
                Ok(false)
            } else {
                Err(e)
            }
        }
    }
}

pub async fn rename_agent(
    secret: &str,
    agent_id: &str,
    name: &str,
) -> crate::Result<()> {
    let _: serde_json::Value = call(
        "/agents/rename",
        Some(secret),
        &json!({ "agent_id": agent_id, "name": name }),
    )
    .await?;
    Ok(())
}

pub async fn delete_tunnel(secret: &str, tunnel_id: &str) -> crate::Result<()> {
    let _: serde_json::Value = call(
        "/tunnels/delete",
        Some(secret),
        &json!({ "tunnel_id": tunnel_id }),
    )
    .await?;
    Ok(())
}

/// Fetches the agent id behind this secret, if playit already knows it
pub async fn agent_id(secret: &str) -> crate::Result<String> {
    #[derive(serde::Deserialize)]
    struct RunData {
        agent_id: String,
    }

    let data: RunData =
        call("/agents/rundata", Some(secret), &json!({})).await?;
    Ok(data.agent_id)
}

/// Polls playit's `/agents/rundata` until the daemon running with this
/// secret has connected and registered itself, returning its agent id
pub async fn wait_for_agent_id(secret: &str) -> crate::Result<String> {
    for _ in 0..30 {
        if let Ok(id) = agent_id(secret).await {
            return Ok(id);
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    Err(crate::ErrorKind::OtherError(
        "Timed out waiting for the playit agent to connect".to_string(),
    )
    .into())
}

pub async fn wait_for_tunnel_address(
    secret: &str,
    tunnel_id: &str,
) -> crate::Result<String> {
    for _ in 0..30 {
        if let Some(address) = tunnel_address(secret, tunnel_id).await? {
            return Ok(address);
        }

        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    Err(crate::ErrorKind::OtherError(
        "Timed out waiting for the playit tunnel to be allocated".to_string(),
    )
    .into())
}

async fn tunnel_address(
    secret: &str,
    tunnel_id: &str,
) -> crate::Result<Option<String>> {
    let list: serde_json::Value = call(
        "/tunnels/list",
        Some(secret),
        &json!({ "tunnel_id": tunnel_id, "agent_id": null }),
    )
    .await?;

    let Some(tunnels) = list.get("tunnels").and_then(|t| t.as_array()) else {
        return Ok(None);
    };

    let Some(tunnel) = tunnels
        .iter()
        .find(|t| t.get("id").and_then(|i| i.as_str()) == Some(tunnel_id))
        .or_else(|| tunnels.first())
    else {
        return Ok(None);
    };

    let alloc = tunnel.get("alloc");
    let status = alloc.and_then(|a| a.get("status")).and_then(|s| s.as_str());

    if status != Some("allocated") {
        return Ok(None);
    }

    let data = alloc.and_then(|a| a.get("data"));
    let host = tunnel
        .get("domain")
        .and_then(|d| d.get("name"))
        .and_then(|n| n.as_str())
        .or_else(|| {
            data.and_then(|d| d.get("assigned_domain"))
                .and_then(|n| n.as_str())
        });
    let port = data
        .and_then(|d| d.get("port_start"))
        .and_then(|p| p.as_u64());

    match (host, port) {
        (Some(host), Some(port)) => Ok(Some(format!("{host}:{port}"))),
        (Some(host), None) => Ok(Some(host.to_string())),
        _ => Ok(None),
    }
}
