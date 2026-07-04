use dashmap::DashMap;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin};
use tokio::sync::Mutex;

pub struct RunningServer {
	pub server: Child,
	pub stdin: Option<Arc<Mutex<ChildStdin>>>,
}

#[derive(Default)]
pub struct HostingManager {
	running: DashMap<String, RunningServer>,
	shared_agent: Mutex<Option<Child>>,
}

impl HostingManager {
	pub fn new() -> Self {
		Self {
			running: DashMap::new(),
			shared_agent: Mutex::new(None),
		}
	}

	pub fn insert(&self, id: String, mut server: Child) {
		let stdin = server.stdin.take().map(|s| Arc::new(Mutex::new(s)));
		self.running.insert(
			id,
			RunningServer {
				server,
				stdin,
			},
		);
	}

	pub async fn set_shared_agent(&self, agent: Child) {
		*self.shared_agent.lock().await = Some(agent);
	}

	pub async fn has_shared_agent(&self) -> bool {
		let mut guard = self.shared_agent.lock().await;
		match guard.as_mut() {
			Some(agent) => matches!(agent.try_wait(), Ok(None)),
			None => false,
		}
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

	pub async fn shared_agent_needs_restart(&self) -> bool {
		let mut guard = self.shared_agent.lock().await;
		match guard.as_mut() {
			Some(agent) => !matches!(agent.try_wait(), Ok(None)),
			None => true,
		}
	}

	pub async fn replace_shared_agent(&self, agent: Child) {
		let mut guard = self.shared_agent.lock().await;
		if let Some(old) = guard.as_mut() {
			let _ = old.start_kill();
		}
		*guard = Some(agent);
	}

	pub async fn shared_agent_pid(&self) -> Option<i64> {
		let guard = self.shared_agent.lock().await;
		guard.as_ref().and_then(|agent| agent.id().map(i64::from))
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

	pub async fn stop(&self, id: &str) -> crate::Result<()> {
		if let Some((_, mut server)) = self.running.remove(id) {
			let _ = server.server.start_kill();
			let _ = server.server.wait().await;
		}

		if self.running.is_empty() {
			self.stop_shared_agent().await;
		}

		Ok(())
	}

	pub async fn stop_shared_agent(&self) {
		let mut guard = self.shared_agent.lock().await;
		if let Some(mut agent) = guard.take() {
			let _ = agent.start_kill();
			let _ = agent.wait().await;
		}
	}

	pub async fn stop_all(&self) {
		let ids: Vec<String> = self
			.running
			.iter()
			.map(|entry| entry.key().clone())
			.collect();
		for id in ids {
			if let Some((_, mut server)) = self.running.remove(&id) {
				let _ = server.server.start_kill();
				let _ = server.server.wait().await;
			}
		}
		self.stop_shared_agent().await;
	}
}
