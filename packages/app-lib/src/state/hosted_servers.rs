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
    /// This install's Minekube Connect endpoint name for this server, e.g.
    /// `kael-abc123def456`. Persisted so the server keeps the same public
    /// address (`<endpoint_name>.play.minekube.net`) across restarts.
    pub endpoint_name: Option<String>,
    pub server_pid: Option<i64>,
    pub gate_pid: Option<i64>,
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
            endpoint_name: row.get("endpoint_name"),
            server_pid: row.get("server_pid"),
            gate_pid: row.get("gate_pid"),
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
                   endpoint_name, server_pid, gate_pid, created, modified
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
                   endpoint_name, server_pid, gate_pid, created, modified
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
                endpoint_name, server_pid, gate_pid, created, modified
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (id) DO UPDATE SET
                name = excluded.name,
                directory = excluded.directory,
                mc_version = excluded.mc_version,
                java_path = excluded.java_path,
                port = excluded.port,
                endpoint_name = excluded.endpoint_name,
                server_pid = excluded.server_pid,
                gate_pid = excluded.gate_pid,
                modified = excluded.modified
            ",
        )
        .bind(&self.id)
        .bind(&self.name)
        .bind(&self.directory)
        .bind(&self.mc_version)
        .bind(&self.java_path)
        .bind(self.port as i64)
        .bind(&self.endpoint_name)
        .bind(self.server_pid)
        .bind(self.gate_pid)
        .bind(self.created)
        .bind(self.modified)
        .execute(exec)
        .await?;

        Ok(())
    }

    pub async fn set_pids(
        id: &str,
        server_pid: Option<i64>,
        gate_pid: Option<i64>,
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<()> {
        sqlx::query(
            "UPDATE hosted_servers SET server_pid = ?, gate_pid = ? WHERE id = ?",
        )
        .bind(server_pid)
        .bind(gate_pid)
        .bind(id)
        .execute(exec)
        .await?;

        Ok(())
    }

    pub async fn set_endpoint_name(
        id: &str,
        endpoint_name: &str,
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<()> {
        sqlx::query("UPDATE hosted_servers SET endpoint_name = ? WHERE id = ?")
            .bind(endpoint_name)
            .bind(id)
            .execute(exec)
            .await?;

        Ok(())
    }

    pub async fn clear_all_pids(
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<()> {
        sqlx::query(
            "UPDATE hosted_servers SET server_pid = NULL, gate_pid = NULL",
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
