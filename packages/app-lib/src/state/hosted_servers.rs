use serde::{Deserialize, Serialize};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

/// Which server jar a hosted server runs, which in turn decides how its Gate
/// tunnel is configured.
///
/// Only Paper speaks Velocity player-info forwarding, so only Paper servers can
/// be given the real Mojang profile that Minekube Connect authenticates. Vanilla
/// servers predate this and stay behind a Lite-mode Gate that pipes raw TCP —
/// pointing a velocity-forwarding Gate at a vanilla backend would break them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerFlavor {
    Vanilla,
    Paper,
}

impl ServerFlavor {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Vanilla => "vanilla",
            Self::Paper => "paper",
        }
    }

    /// Unknown values decay to `Vanilla`, the conservative choice: it renders
    /// the Lite-mode Gate config that every pre-existing server already runs.
    fn from_str(value: &str) -> Self {
        match value {
            "paper" => Self::Paper,
            _ => Self::Vanilla,
        }
    }
}

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
    pub flavor: ServerFlavor,
    /// Player cap. Written into `server.properties` and mirrored into Gate's
    /// status ping, so it lives here rather than only in the properties file.
    pub max_players: u32,
    /// Chunk render radius. The dominant cost on the host's upload link, so it
    /// is exposed as an editable setting.
    pub view_distance: u32,
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
            flavor: ServerFlavor::from_str(row.get("flavor")),
            max_players: row.get::<i64, _>("max_players") as u32,
            view_distance: row.get::<i64, _>("view_distance") as u32,
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
                   endpoint_name, flavor, max_players, view_distance,
                   server_pid, gate_pid, created, modified
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
                   endpoint_name, flavor, max_players, view_distance,
                   server_pid, gate_pid, created, modified
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
                endpoint_name, flavor, max_players, view_distance,
                server_pid, gate_pid, created, modified
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT (id) DO UPDATE SET
                name = excluded.name,
                directory = excluded.directory,
                mc_version = excluded.mc_version,
                java_path = excluded.java_path,
                port = excluded.port,
                endpoint_name = excluded.endpoint_name,
                flavor = excluded.flavor,
                max_players = excluded.max_players,
                view_distance = excluded.view_distance,
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
        .bind(self.flavor.as_str())
        .bind(self.max_players as i64)
        .bind(self.view_distance as i64)
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

    pub async fn set_port(
        id: &str,
        port: u16,
        exec: impl sqlx::Executor<'_, Database = sqlx::Sqlite>,
    ) -> crate::Result<()> {
        sqlx::query("UPDATE hosted_servers SET port = ? WHERE id = ?")
            .bind(port as i64)
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
