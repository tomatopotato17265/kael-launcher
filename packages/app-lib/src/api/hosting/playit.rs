use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;

const API_BASE: &str = "https://api.playit.gg";

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

pub fn begin_claim() -> ClaimInfo {
    let code: String =
        (0..5).map(|_| format!("{:02x}", rand::random::<u8>())).collect();
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
            "version": "kael 1.0",
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

pub async fn create_tunnel(secret: &str, name: &str) -> crate::Result<String> {
    #[derive(serde::Deserialize)]
    struct ObjectId {
        id: String,
    }

    let req = json!({
        "name": name,
        "tunnel_type": "minecraft-java",
        "port_type": "tcp",
        "port_count": 1,
        "origin": { "type": "managed", "agent_id": null },
        "enabled": true,
        "alloc": null,
        "firewall_id": null,
        "proxy_protocol": null,
    });

    let object: ObjectId = call("/tunnels/create", Some(secret), &req).await?;
    Ok(object.id)
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

    let tunnels = match list.get("tunnels").and_then(|t| t.as_array()) {
        Some(tunnels) => tunnels,
        None => return Ok(None),
    };

    let tunnel = match tunnels
        .iter()
        .find(|t| t.get("id").and_then(|i| i.as_str()) == Some(tunnel_id))
        .or_else(|| tunnels.first())
    {
        Some(tunnel) => tunnel,
        None => return Ok(None),
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
