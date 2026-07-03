use dashmap::DashMap;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin};
use tokio::sync::Mutex;

pub struct RunningServer {
    pub server: Child,
    pub agent: Child,
    pub stdin: Option<Arc<Mutex<ChildStdin>>>,
}

#[derive(Default)]
pub struct HostingManager {
    running: DashMap<String, RunningServer>,
}

impl HostingManager {
    pub fn new() -> Self {
        Self {
            running: DashMap::new(),
        }
    }

    pub fn insert(&self, id: String, mut server: Child, agent: Child) {
        let stdin = server.stdin.take().map(|s| Arc::new(Mutex::new(s)));
        self.running.insert(
            id,
            RunningServer {
                server,
                agent,
                stdin,
            },
        );
    }

    pub async fn send_command(
        &self,
        id: &str,
        command: &str,
    ) -> crate::Result<()> {
        // Clone the Arc out before awaiting so no DashMap guard is held
        // across an await point
        let stdin = self
            .running
            .get(id)
            .and_then(|entry| entry.stdin.clone())
            .ok_or_else(|| {
                crate::ErrorKind::InputError(format!(
                    "Server {id} is not running"
                ))
            })?;

        let mut stdin = stdin.lock().await;
        stdin
            .write_all(format!("{command}\n").as_bytes())
            .await
            .map_err(crate::util::io::IOError::from)?;
        stdin
            .flush()
            .await
            .map_err(crate::util::io::IOError::from)?;
        Ok(())
    }

    pub fn is_running(&self, id: &str) -> bool {
        let exited = match self.running.get_mut(id) {
            Some(mut entry) => matches!(entry.server.try_wait(), Ok(Some(_))),
            None => return false,
        };

        if exited {
            self.force_remove(id);
            return false;
        }

        true
    }

    /// Whether the server is running but its playit daemon has exited (or
    /// was never kept alive) and needs to be respawned
    pub fn agent_needs_restart(&self, id: &str) -> bool {
        match self.running.get_mut(id) {
            Some(mut entry) => !matches!(entry.agent.try_wait(), Ok(None)),
            None => false,
        }
    }

    /// Replaces a dead playit daemon child with a freshly spawned one
    pub fn replace_agent(&self, id: &str, agent: Child) {
        if let Some(mut entry) = self.running.get_mut(id) {
            let _ = entry.agent.start_kill();
            entry.agent = agent;
        }
    }

    pub fn running_ids(&self) -> Vec<String> {
        let ids: Vec<String> = self
            .running
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        ids.into_iter().filter(|id| self.is_running(id)).collect()
    }

    fn force_remove(&self, id: &str) {
        if let Some((_, mut server)) = self.running.remove(id) {
            let _ = server.agent.start_kill();
        }
    }

    pub async fn stop(&self, id: &str) -> crate::Result<()> {
        if let Some((_, mut server)) = self.running.remove(id) {
            let _ = server.server.start_kill();
            let _ = server.agent.start_kill();
            let _ = server.server.wait().await;
            let _ = server.agent.wait().await;
        }

        Ok(())
    }

    pub async fn stop_all(&self) {
        let ids: Vec<String> = self
            .running
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        for id in ids {
            let _ = self.stop(&id).await;
        }
    }
}
