use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;

const API_BASE: &str = "https://api.cloudflare.com/client/v4";
const DEFAULT_DOMAIN: &str = "kaelmc.net";

pub struct CloudflareConfig {
    pub token: String,
    pub zone_id: String,
    pub domain: String,
}

/// Reads Cloudflare credentials from the environment. Returns `None` when
/// they are absent, in which case custom domains are skipped entirely and
/// servers keep their plain playit address.
pub fn config_from_env() -> Option<CloudflareConfig> {
    let token = std::env::var("CLOUDFLARE_API_TOKEN").ok()?;
    let zone_id = std::env::var("CLOUDFLARE_ZONE_ID").ok()?;

    if token.trim().is_empty() || zone_id.trim().is_empty() {
        return None;
    }

    Some(CloudflareConfig {
        token: token.trim().to_string(),
        zone_id: zone_id.trim().to_string(),
        domain: std::env::var("KAELMC_DOMAIN")
            .ok()
            .map(|d| d.trim().trim_matches('.').to_string())
            .filter(|d| !d.is_empty())
            .unwrap_or_else(|| DEFAULT_DOMAIN.to_string()),
    })
}

#[derive(Serialize, serde::Deserialize)]
pub struct DnsRecordIds {
    pub srv: String,
    pub a: String,
}

#[derive(serde::Deserialize)]
struct ApiEnvelope<T> {
    success: bool,
    #[serde(default)]
    errors: serde_json::Value,
    result: Option<T>,
}

async fn call<Req: Serialize, Res: DeserializeOwned>(
    cfg: &CloudflareConfig,
    method: reqwest::Method,
    path: &str,
    body: Option<&Req>,
) -> crate::Result<Res> {
    let client = reqwest::Client::new();
    let mut builder = client
        .request(method, format!("{API_BASE}{path}"))
        .bearer_auth(&cfg.token);

    if let Some(body) = body {
        builder = builder.json(body);
    }

    let text = builder.send().await?.text().await?;
    let parsed: ApiEnvelope<Res> =
        serde_json::from_str(&text).map_err(|e| {
            crate::ErrorKind::OtherError(format!(
                "Failed to parse Cloudflare response ({path}): {e}: {text}"
            ))
        })?;

    if !parsed.success {
        return Err(crate::ErrorKind::OtherError(format!(
            "Cloudflare request failed ({path}): {}",
            parsed.errors
        ))
        .into());
    }

    parsed.result.ok_or_else(|| {
        crate::ErrorKind::OtherError(format!(
            "Cloudflare response had no result ({path})"
        ))
        .into()
    })
}

/// Resolves a hostname's IPv4 address via DNS-over-HTTPS. playit's tunnel
/// hostnames carry both an A and an AAAA record, and clients that pick the
/// AAAA record on a network with broken IPv6 routing hang forever - so
/// instead of a CNAME (which inherits playit's dual-stack answer) we pin our
/// own domain to a plain IPv4 address we resolve ourselves.
async fn resolve_ipv4(host: &str) -> crate::Result<String> {
    #[derive(serde::Deserialize)]
    struct DohAnswer {
        #[serde(rename = "type")]
        record_type: u16,
        data: String,
    }

    #[derive(serde::Deserialize)]
    struct DohResponse {
        #[serde(default)]
        #[serde(rename = "Answer")]
        answer: Vec<DohAnswer>,
    }

    let client = reqwest::Client::new();
    let response: DohResponse = client
        .get("https://cloudflare-dns.com/dns-query")
        .query(&[("name", host), ("type", "A")])
        .header("accept", "application/dns-json")
        .send()
        .await?
        .json()
        .await
        .map_err(|e| {
            crate::ErrorKind::OtherError(format!(
                "Failed to resolve {host}: {e}"
            ))
        })?;

    response
        .answer
        .into_iter()
        .find(|a| a.record_type == 1)
        .map(|a| a.data)
        .ok_or_else(|| {
            crate::ErrorKind::OtherError(format!("{host} has no IPv4 address"))
                .into()
        })
}

/// Creates the DNS records that make `<subdomain>.<domain>` join the playit
/// tunnel: an A record pinned to playit's resolved IPv4 address, plus an SRV
/// record (Minecraft clients resolve `_minecraft._tcp`, which carries the
/// tunnel's port) pointing back at that same A record. Returns the chosen
/// FQDN and the record ids for later cleanup.
pub async fn create_server_records(
    cfg: &CloudflareConfig,
    server_name: &str,
    target_host: &str,
    port: u16,
) -> crate::Result<(String, DnsRecordIds)> {
    let ipv4 = resolve_ipv4(target_host).await?;

    let mut subdomain = sanitize_subdomain(server_name);
    let mut fqdn = format!("{subdomain}.{}", cfg.domain);

    if record_name_taken(cfg, &fqdn).await? {
        let suffix: String = (0..4)
            .map(|_| {
                let n = rand::random::<u8>() % 36;
                char::from_digit(n as u32, 36).unwrap_or('0')
            })
            .collect();
        subdomain = format!("{subdomain}-{suffix}");
        fqdn = format!("{subdomain}.{}", cfg.domain);

        if record_name_taken(cfg, &fqdn).await? {
            return Err(crate::ErrorKind::OtherError(format!(
                "Could not find a free subdomain for {fqdn}"
            ))
            .into());
        }
    }

    #[derive(serde::Deserialize)]
    struct CreatedRecord {
        id: String,
    }

    let a: CreatedRecord = call(
        cfg,
        reqwest::Method::POST,
        &format!("/zones/{}/dns_records", cfg.zone_id),
        Some(&json!({
            "type": "A",
            "name": fqdn,
            "content": ipv4,
            "proxied": false,
            "ttl": 1,
        })),
    )
    .await?;

    let srv_result: crate::Result<CreatedRecord> = call(
        cfg,
        reqwest::Method::POST,
        &format!("/zones/{}/dns_records", cfg.zone_id),
        Some(&json!({
            "type": "SRV",
            "name": format!("_minecraft._tcp.{fqdn}"),
            "data": {
                "priority": 0,
                "weight": 5,
                "port": port,
                "target": fqdn,
            },
            "ttl": 1,
        })),
    )
    .await;

    let srv = match srv_result {
        Ok(record) => record,
        Err(e) => {
            let _ = delete_record(cfg, &a.id).await;
            return Err(e);
        }
    };

    Ok((
        fqdn,
        DnsRecordIds {
            srv: srv.id,
            a: a.id,
        },
    ))
}

pub async fn delete_server_records(cfg: &CloudflareConfig, ids: &DnsRecordIds) {
    for id in [&ids.srv, &ids.a] {
        if let Err(e) = delete_record(cfg, id).await {
            tracing::warn!("Failed to delete Cloudflare DNS record {id}: {e}");
        }
    }
}

async fn delete_record(
    cfg: &CloudflareConfig,
    record_id: &str,
) -> crate::Result<()> {
    let _: serde_json::Value = call::<(), _>(
        cfg,
        reqwest::Method::DELETE,
        &format!("/zones/{}/dns_records/{record_id}", cfg.zone_id),
        None,
    )
    .await?;
    Ok(())
}

async fn record_name_taken(
    cfg: &CloudflareConfig,
    fqdn: &str,
) -> crate::Result<bool> {
    let records: Vec<serde_json::Value> = call::<(), _>(
        cfg,
        reqwest::Method::GET,
        &format!("/zones/{}/dns_records?name={fqdn}", cfg.zone_id),
        None,
    )
    .await?;

    Ok(!records.is_empty())
}

fn sanitize_subdomain(name: &str) -> String {
    let mut result = String::new();
    let mut last_dash = true;

    for c in name.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            result.push(c);
            last_dash = false;
        } else if !last_dash {
            result.push('-');
            last_dash = true;
        }

        if result.len() >= 40 {
            break;
        }
    }

    let trimmed = result.trim_matches('-');
    if trimmed.is_empty() {
        "server".to_string()
    } else {
        trimmed.to_string()
    }
}
