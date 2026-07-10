use dashmap::DashMap;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin};
use tokio::sync::Mutex;

pub struct RunningServer {
    pub server: Child,
    pub stdin: Option<Arc<Mutex<ChildStdin>>>,
}

/// Tracks each hosted server's Minecraft process and its own Gate tunnel
/// process. Unlike playit's single shared agent, every server links its own
/// Connect endpoint, so there is one Gate child per running server rather
/// than one shared daemon for all of them.
#[derive(Default)]
pub struct HostingManager {
    running: DashMap<String, RunningServer>,
    gates: DashMap<String, Child>,
}

impl HostingManager {
    pub fn new() -> Self {
        Self {
            running: DashMap::new(),
            gates: DashMap::new(),
        }
    }

    pub fn insert(&self, id: String, mut server: Child) {
        let stdin = server.stdin.take().map(|s| Arc::new(Mutex::new(s)));
        self.running.insert(id, RunningServer { server, stdin });
    }

    pub fn insert_gate(&self, id: String, gate: Child) {
        if let Some((_, mut old)) = self.gates.remove(&id) {
            let _ = old.start_kill();
        }
        self.gates.insert(id, gate);
    }

    pub fn is_gate_running(&self, id: &str) -> bool {
        let exited = match self.gates.get_mut(id) {
            Some(mut entry) => matches!(entry.try_wait(), Ok(Some(_))),
            None => return false,
        };

        if exited {
            self.gates.remove(id);
            return false;
        }

        true
    }

    pub async fn send_command(
        &self,
        id: &str,
        command: &str,
    ) -> crate::Result<()> {
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

    pub fn running_ids(&self) -> Vec<String> {
        let ids: Vec<String> = self
            .running
            .iter()
            .map(|entry| entry.key().clone())
            .collect();
        ids.into_iter().filter(|id| self.is_running(id)).collect()
    }

    fn force_remove(&self, id: &str) {
        self.running.remove(id);
    }

    async fn stop_gate(&self, id: &str) {
        if let Some((_, mut gate)) = self.gates.remove(id) {
            let _ = gate.start_kill();
            let _ = gate.wait().await;
        }
    }

    pub async fn stop(&self, id: &str) -> crate::Result<()> {
        if let Some((_, mut server)) = self.running.remove(id) {
            let _ = server.server.start_kill();
            let _ = server.server.wait().await;
        }

        self.stop_gate(id).await;

        Ok(())
    }

    pub async fn stop_all(&self) {
        let ids: Vec<String> = self
            .running
            .iter()
            .map(|entry| entry.key().clone())
            .chain(self.gates.iter().map(|entry| entry.key().clone()))
            .collect();
        for id in ids {
            if let Some((_, mut server)) = self.running.remove(&id) {
                let _ = server.server.start_kill();
                let _ = server.server.wait().await;
            }
            self.stop_gate(&id).await;
        }
    }
}
