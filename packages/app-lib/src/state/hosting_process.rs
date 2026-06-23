use dashmap::DashMap;
use tokio::process::Child;

pub struct RunningServer {
    pub server: Child,
    pub agent: Child,
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

    pub fn insert(&self, id: String, server: Child, agent: Child) {
        self.running.insert(id, RunningServer { server, agent });
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
        let ids: Vec<String> =
            self.running.iter().map(|entry| entry.key().clone()).collect();
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
}
