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

/// Reads a setting from the runtime environment, falling back to the value
/// baked in at compile time from `packages/app-lib/.env` (see build.rs)
fn setting(
    runtime_name: &str,
    compiled: Option<&'static str>,
) -> Option<String> {
    std::env::var(runtime_name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| {
            compiled
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        })
}

/// Reads Cloudflare credentials from the runtime environment or the values
/// compiled in from `packages/app-lib/.env`. Returns `None` when they are
/// absent, in which case custom domains are skipped entirely and servers
/// keep their plain playit address.
pub fn config_from_env() -> Option<CloudflareConfig> {
    let token =
        setting("CLOUDFLARE_API_TOKEN", option_env!("CLOUDFLARE_API_TOKEN"))?;
    let zone_id =
        setting("CLOUDFLARE_ZONE_ID", option_env!("CLOUDFLARE_ZONE_ID"))?;

    Some(CloudflareConfig {
        token,
        zone_id,
        domain: domain_from_env(),
    })
}

fn domain_from_env() -> String {
    setting("KAELMC_DOMAIN", option_env!("KAELMC_DOMAIN"))
        .map(|d| d.trim_matches('.').to_string())
        .filter(|d| !d.is_empty())
        .unwrap_or_else(|| DEFAULT_DOMAIN.to_string())
}

/// How the app manages `<name>.<domain>` DNS records. The Worker variant is
/// the distribution-safe path: the Cloudflare token lives only inside the
/// kaelmc-dns Worker (see `packages/app-lib/cloudflare-worker/`), the app
/// authenticates with an anonymous per-install owner key, and subdomain
/// ownership is enforced server-side. The Direct variant talks straight to
/// the Cloudflare API and only makes sense for local development.
pub enum DnsBackend {
    Worker {
        url: String,
        owner_key: String,
        domain: String,
    },
    Direct(CloudflareConfig),
}

impl DnsBackend {
    pub fn domain(&self) -> &str {
        match self {
            Self::Worker { domain, .. } => domain,
            Self::Direct(cfg) => &cfg.domain,
        }
    }
}

pub fn backend_from_env(owner_key: Option<String>) -> Option<DnsBackend> {
    if let Some(url) =
        setting("KAELMC_WORKER_URL", option_env!("KAELMC_WORKER_URL"))
    {
        return Some(DnsBackend::Worker {
            url: url.trim_end_matches('/').to_string(),
            owner_key: owner_key?,
            domain: domain_from_env(),
        });
    }

    config_from_env().map(DnsBackend::Direct)
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

/// Creates (or refreshes) the DNS records that make `<subdomain>.<domain>`
/// join the playit tunnel: an A record pinned to playit's resolved IPv4
/// address, plus an SRV record (Minecraft clients resolve `_minecraft._tcp`,
/// which carries the tunnel's port) pointing back at that same A record.
/// Returns the chosen FQDN and, in direct mode, the record ids for cleanup.
pub async fn create_server_records(
    backend: &DnsBackend,
    server_name: &str,
    target_host: &str,
    port: u16,
    reserved: &[String],
    pinned_ipv4: Option<&str>,
) -> crate::Result<(String, Option<DnsRecordIds>)> {
    let ipv4 = match pinned_ipv4 {
        Some(ip) => ip.to_owned(),
        None => resolve_ipv4(target_host).await?,
    };

    match backend {
        DnsBackend::Worker {
            url,
            owner_key,
            domain,
        } => {
            // Cross-user conflicts are handled by the worker; only same-
            // install conflicts (two of the user's own servers sharing a
            // name) need handling here, since the worker sees one owner
            let mut subdomain = sanitize_subdomain(server_name);
            if reserved
                .iter()
                .any(|r| r == &format!("{subdomain}.{domain}"))
            {
                subdomain = format!("{subdomain}-{}", random_suffix());
            }

            let fqdn =
                worker_claim(url, owner_key, &subdomain, &ipv4, port).await?;
            Ok((fqdn, None))
        }
        DnsBackend::Direct(cfg) => {
            let (fqdn, ids) =
                direct_create(cfg, server_name, &ipv4, port, reserved).await?;
            Ok((fqdn, Some(ids)))
        }
    }
}

/// Removes a server's DNS records and, in worker mode, releases its
/// subdomain claim. Best-effort: failures are logged, never propagated.
pub async fn release_server_records(
    backend: &DnsBackend,
    custom_domain: &str,
    ids: Option<&DnsRecordIds>,
) {
    match backend {
        DnsBackend::Worker { url, owner_key, .. } => {
            let suffix = format!(".{}", backend.domain());
            let Some(subdomain) = custom_domain.strip_suffix(&suffix) else {
                return;
            };
            if let Err(e) = worker_release(url, owner_key, subdomain).await {
                tracing::warn!(
                    "Failed to release subdomain {custom_domain}: {e}"
                );
            }
        }
        DnsBackend::Direct(cfg) => {
            if let Some(ids) = ids {
                delete_server_records(cfg, ids).await;
            }
        }
    }
}

#[derive(serde::Deserialize)]
struct WorkerResponse {
    fqdn: Option<String>,
    error: Option<String>,
    #[allow(dead_code)]
    ok: Option<bool>,
}

async fn worker_call(
    url: &str,
    path: &str,
    body: &serde_json::Value,
) -> crate::Result<WorkerResponse> {
    let response = reqwest::Client::new()
        .post(format!("{url}{path}"))
        .json(body)
        .send()
        .await?;

    let status = response.status();
    let text = response.text().await?;
    serde_json::from_str(&text).map_err(|_| {
        crate::ErrorKind::OtherError(format!(
            "Unexpected DNS worker response ({path}, {status}): {text}"
        ))
        .into()
    })
}

async fn worker_claim(
    url: &str,
    owner_key: &str,
    subdomain: &str,
    ip: &str,
    port: u16,
) -> crate::Result<String> {
    let response = worker_call(
        url,
        "/claim",
        &json!({
            "subdomain": subdomain,
            "ip": ip,
            "port": port,
            "owner": owner_key,
        }),
    )
    .await?;

    response.fqdn.ok_or_else(|| {
        crate::ErrorKind::OtherError(format!(
            "The DNS worker refused the claim: {}",
            response
                .error
                .unwrap_or_else(|| "unknown error".to_string())
        ))
        .into()
    })
}

async fn worker_release(
    url: &str,
    owner_key: &str,
    subdomain: &str,
) -> crate::Result<()> {
    let response = worker_call(
        url,
        "/release",
        &json!({ "subdomain": subdomain, "owner": owner_key }),
    )
    .await?;

    match response.error {
        Some(error) => Err(crate::ErrorKind::OtherError(format!(
            "The DNS worker refused the release: {error}"
        ))
        .into()),
        None => Ok(()),
    }
}

fn random_suffix() -> String {
    (0..4)
        .map(|_| {
            let n = rand::random::<u8>() % 36;
            char::from_digit(n as u32, 36).unwrap_or('0')
        })
        .collect()
}

/// Direct-mode record creation. A name already present in the zone only
/// forces a `-suffix` when it is in active use: either another local server
/// owns it (`reserved`), or a Minecraft server actually answers behind its
/// records. Dead leftovers (e.g. from a cleanup that failed) are deleted and
/// the name reclaimed, so a name's *previous* use never costs a new server
/// its natural subdomain.
async fn direct_create(
    cfg: &CloudflareConfig,
    server_name: &str,
    ipv4: &str,
    port: u16,
    reserved: &[String],
) -> crate::Result<(String, DnsRecordIds)> {
    let mut subdomain = sanitize_subdomain(server_name);
    let mut fqdn = format!("{subdomain}.{}", cfg.domain);

    if !name_available(cfg, &fqdn, reserved).await? {
        subdomain = format!("{subdomain}-{}", random_suffix());
        fqdn = format!("{subdomain}.{}", cfg.domain);

        if !name_available(cfg, &fqdn, reserved).await? {
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

async fn records_named(
    cfg: &CloudflareConfig,
    name: &str,
) -> crate::Result<Vec<serde_json::Value>> {
    call::<(), _>(
        cfg,
        reqwest::Method::GET,
        &format!("/zones/{}/dns_records?name={name}", cfg.zone_id),
        None,
    )
    .await
}

/// Decides whether `fqdn` can be used for a new server. Existing records
/// only make it unavailable when they are in active use; dead leftovers are
/// deleted so the name frees up again.
async fn name_available(
    cfg: &CloudflareConfig,
    fqdn: &str,
    reserved: &[String],
) -> crate::Result<bool> {
    if reserved.iter().any(|r| r == fqdn) {
        return Ok(false);
    }

    let a_records = records_named(cfg, fqdn).await?;
    let srv_records =
        records_named(cfg, &format!("_minecraft._tcp.{fqdn}")).await?;

    if a_records.is_empty() && srv_records.is_empty() {
        return Ok(true);
    }

    let target_ip = a_records
        .iter()
        .find(|r| r.get("type").and_then(|t| t.as_str()) == Some("A"))
        .and_then(|r| r.get("content"))
        .and_then(|c| c.as_str());
    let srv_port = srv_records
        .iter()
        .find(|r| r.get("type").and_then(|t| t.as_str()) == Some("SRV"))
        .and_then(|r| r.get("data"))
        .and_then(|d| d.get("port"))
        .and_then(|p| p.as_u64())
        .and_then(|p| u16::try_from(p).ok());

    if let (Some(ip), Some(port)) = (target_ip, srv_port)
        && super::minecraft_responds(ip, port).await
    {
        return Ok(false);
    }

    tracing::info!(
        "Reclaiming {fqdn}: its DNS records exist but nothing answers behind them"
    );
    for record in a_records.iter().chain(srv_records.iter()) {
        if let Some(id) = record.get("id").and_then(|i| i.as_str())
            && let Err(e) = delete_record(cfg, id).await
        {
            tracing::warn!("Failed to delete stale record {id}: {e}");
            return Ok(false);
        }
    }

    Ok(true)
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
