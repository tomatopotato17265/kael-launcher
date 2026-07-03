use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostedServer {
    pub id: String,
    pub name: String,
    pub directory: String,
    pub mc_version: String,
    pub java_path: Option<String>,
    pub port: u16,
    pub playit_tunnel_id: Option<String>,
    pub tunnel_url: Option<String>,
    pub custom_domain: Option<String>,
    pub cf_record_ids: Option<String>,
    pub server_pid: Option<i64>,
    pub agent_pid: Option<i64>,
    pub created: i64,
    pub modified: i64,
}

impl HostedServer {
    fn from_row(row: SqliteRow) -> Self {
        Self {
            id: row.get("id"),
            name: row.get("name"),
            directory: row.get("directory"),
            mc_version: row.get("mc_version"),
            java_path: row.get("java_path"),
            port: row.get::<i64, _>("port") as u16,
            playit_tunnel_id: row.get("playit_tunnel_id"),
            tunnel_url: row.get("tunnel_url"),
            custom_domain: row.get("custom_domain"),
            cf_record_ids: row.get("cf_record_ids"),
            server_pid: row.get("server_pid"),
            agent_pid: row.get("agent_pid"),
            created: row.get("created"),
            modified: row.get("modified"),
        }
    }

    pub async fn get(
        id: &str,
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<Option<Self>> {
        let row = sqlx::query(
            "
            SELECT id, name, directory, mc_version, java_path, port,
                   playit_tunnel_id, tunnel_url, custom_domain, cf_record_ids,
                   server_pid, agent_pid, created, modified
            FROM hosted_servers
            WHERE id = ?
            ",
        )
        .bind(id)
        .fetch_optional(exec)
        .await?;

        Ok(row.map(Self::from_row))
    }

    pub async fn get_all(
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<Vec<Self>> {
        let rows = sqlx::query(
            "
            SELECT id, name, directory, mc_version, java_path, port,
                   playit_tunnel_id, tunnel_url, custom_domain, cf_record_ids,
                   server_pid, agent_pid, created, modified
            FROM hosted_servers
            ORDER BY created ASC
            ",
        )
        .fetch_all(exec)
        .await?;

        Ok(rows.into_iter().map(Self::from_row).collect())
    }

    pub async fn upsert(
        &self,
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<()> {
        sqlx::query(
            "
            INSERT INTO hosted_servers (
                id, name, directory, mc_version, java_path, port,
                playit_tunnel_id, tunnel_url, custom_domain, cf_record_ids,
                server_pid, agent_pid, created, modified
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (id) DO UPDATE SET
                name = excluded.name,
                directory = excluded.directory,
                mc_version = excluded.mc_version,
                java_path = excluded.java_path,
                port = excluded.port,
                playit_tunnel_id = excluded.playit_tunnel_id,
                tunnel_url = excluded.tunnel_url,
                custom_domain = excluded.custom_domain,
                cf_record_ids = excluded.cf_record_ids,
                server_pid = excluded.server_pid,
                agent_pid = excluded.agent_pid,
                modified = excluded.modified
            ",
        )
        .bind(&self.id)
        .bind(&self.name)
        .bind(&self.directory)
        .bind(&self.mc_version)
        .bind(&self.java_path)
        .bind(self.port as i64)
        .bind(&self.playit_tunnel_id)
        .bind(&self.tunnel_url)
        .bind(&self.custom_domain)
        .bind(&self.cf_record_ids)
        .bind(self.server_pid)
        .bind(self.agent_pid)
        .bind(self.created)
        .bind(self.modified)
        .execute(exec)
        .await?;

        Ok(())
    }

    pub async fn set_pids(
        id: &str,
        server_pid: Option<i64>,
        agent_pid: Option<i64>,
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<()> {
        sqlx::query(
            "UPDATE hosted_servers SET server_pid = ?, agent_pid = ? WHERE id = ?",
        )
        .bind(server_pid)
        .bind(agent_pid)
        .bind(id)
        .execute(exec)
        .await?;

        Ok(())
    }

    pub async fn clear_all_pids(
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<()> {
        sqlx::query(
            "UPDATE hosted_servers SET server_pid = NULL, agent_pid = NULL",
        )
        .execute(exec)
        .await?;

        Ok(())
    }

    /// Clears the tunnel and custom-domain state of every server, used when
    /// the playit agent they belonged to no longer exists
    pub async fn clear_all_tunnels(
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<()> {
        sqlx::query(
            "
            UPDATE hosted_servers SET
                playit_tunnel_id = NULL,
                tunnel_url = NULL,
                custom_domain = NULL,
                cf_record_ids = NULL
            ",
        )
        .execute(exec)
        .await?;

        Ok(())
    }

    pub async fn remove(
        id: &str,
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<()> {
        sqlx::query("DELETE FROM hosted_servers WHERE id = ?")
            .bind(id)
            .execute(exec)
            .await?;

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayitAccount {
    pub secret_key: String,
    pub account_type: String,
}

impl PlayitAccount {
    pub async fn get(
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<Option<Self>> {
        let row = sqlx::query(
            "SELECT secret_key, account_type FROM playit_account WHERE id = 0",
        )
        .fetch_optional(exec)
        .await?;

        Ok(row.map(|row| Self {
            secret_key: row.get("secret_key"),
            account_type: row.get("account_type"),
        }))
    }

    pub async fn remove(
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<()> {
        sqlx::query("DELETE FROM playit_account WHERE id = 0")
            .execute(exec)
            .await?;

        Ok(())
    }

    pub async fn upsert(
        &self,
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<()> {
        sqlx::query(
            "
            INSERT INTO playit_account (id, secret_key, account_type)
            VALUES (0, ?, ?)
            ON CONFLICT (id) DO UPDATE SET
                secret_key = excluded.secret_key,
                account_type = excluded.account_type
            ",
        )
        .bind(&self.secret_key)
        .bind(&self.account_type)
        .execute(exec)
        .await?;

        Ok(())
    }
}

/// Anonymous per-install identity used to claim kaelmc subdomains through
/// the DNS worker; generated once and kept for the lifetime of the install
pub struct DnsOwner;

impl DnsOwner {
    pub async fn get_or_create(
        pool: &sqlx::SqlitePool,
    ) -> crate::Result<String> {
        let row = sqlx::query("SELECT owner_key FROM dns_owner WHERE id = 0")
            .fetch_optional(pool)
            .await?;

        if let Some(row) = row {
            return Ok(row.get("owner_key"));
        }

        let key = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO dns_owner (id, owner_key) VALUES (0, ?)")
            .bind(&key)
            .execute(pool)
            .await?;

        Ok(key)
    }
}
