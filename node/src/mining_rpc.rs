//! External mining RPC.
//!
//! Lets miners running outside the node pull or subscribe to the current mining
//! task and submit a seal they found. The node revalidates every submission
//! through the regular import path, so external compute is never trusted. The
//! seal carries only the nonce and work; the reward address is pinned in the
//! pre-runtime digest the node builds, so an external miner cannot redirect the
//! reward.

use std::sync::Arc;

use jsonrpsee::{
	core::{
		async_trait,
		server::{PendingSubscriptionSink, SubscriptionMessage},
		RpcResult, SubscriptionResult,
	},
	proc_macros::rpc,
	types::ErrorObjectOwned,
};
use serde::{Deserialize, Serialize};
use sp_core::{Bytes, H256, U256};
use tokio::sync::watch;

/// No mining task exists yet (worker just started or syncing).
const NO_TASK: i32 = 9001;

/// The blocking import task failed to run to completion.
const SUBMIT_FAILED: i32 = 9002;

/// The mining worker as the RPC sees it, reading the live task and handing a seal
/// back for import. Keeps the consensus generics out of the RPC layer.
///
/// `submit_seal` blocks on the full import, so callers must run it off the async
/// executor.
pub trait ExternalMiner: Send + Sync {
	/// Pre-hash and difficulty of the most recent task, or `None` before the
	/// first build lands.
	fn current_task(&self) -> Option<(H256, U256)>;

	/// Revalidate and import the seal found for `pre_hash`. Returns `true` when
	/// the block imports. Any task still tied to the current head is accepted,
	/// new or old; a `pre_hash` the head has moved past is gone, so it returns
	/// `false`.
	fn submit_seal(&self, pre_hash: H256, seal: Vec<u8>) -> bool;

	/// A receiver that wakes on every task change, letting the subscription await
	/// the next task instead of polling.
	fn task_changes(&self) -> watch::Receiver<u64>;
}

/// Everything an external miner needs to compute the work for one task.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MiningTask {
	/// Block pre-hash the seal must be mined against.
	pub pre_hash: H256,
	/// Difficulty target the resulting work must satisfy.
	pub difficulty: U256,
	/// Reward and seed-bound miner address, in SS58.
	pub miner: String,
	/// Protocol string pinning the algorithm, resolution and quantization.
	pub protocol: String,
}

/// RPC surface for external miners.
#[rpc(server)]
pub trait MiningApi {
	/// Pull the current mining task.
	#[method(name = "mining_getTask")]
	fn get_task(&self) -> RpcResult<MiningTask>;

	/// Stream the current task, pushing a fresh one on every new-task tick.
	#[subscription(
		name = "mining_subscribeTask" => "mining_task",
		unsubscribe = "mining_unsubscribeTask",
		item = MiningTask
	)]
	async fn subscribe_task(&self) -> SubscriptionResult;

	/// Submit a seal found for `pre_hash`. Any task still live on the current
	/// head is accepted; a submission the head has moved past returns `false`.
	#[method(name = "mining_submitSeal")]
	async fn submit_seal(&self, pre_hash: H256, seal: Bytes) -> RpcResult<bool>;
}

/// Mining RPC backed by the node's PoW worker.
pub struct Mining {
	handle: Arc<dyn ExternalMiner>,
	miner: String,
	protocol: String,
}

impl Mining {
	/// Build the RPC over a worker handle, the configured reward address and the
	/// active protocol string.
	pub fn new(handle: Arc<dyn ExternalMiner>, miner: String, protocol: String) -> Self {
		Self { handle, miner, protocol }
	}
}

/// Snapshot the worker's current task into a wire `MiningTask`, if one exists.
fn task_of(handle: &dyn ExternalMiner, miner: &str, protocol: &str) -> Option<MiningTask> {
	handle.current_task().map(|(pre_hash, difficulty)| MiningTask {
		pre_hash,
		difficulty,
		miner: miner.to_owned(),
		protocol: protocol.to_owned(),
	})
}

#[async_trait]
impl MiningApiServer for Mining {
	fn get_task(&self) -> RpcResult<MiningTask> {
		task_of(&*self.handle, &self.miner, &self.protocol).ok_or_else(no_task)
	}

	async fn subscribe_task(&self, pending: PendingSubscriptionSink) -> SubscriptionResult {
		let sink = pending.accept().await?;
		// Subscribe before the first read so a task built in the gap still wakes us.
		let mut changes = self.handle.task_changes();
		loop {
			if let Some(task) = task_of(&*self.handle, &self.miner, &self.protocol)
				&& sink.send(SubscriptionMessage::from_json(&task)?).await.is_err()
			{
				break;
			}
			if changes.changed().await.is_err() {
				break;
			}
		}
		Ok(())
	}

	async fn submit_seal(&self, pre_hash: H256, seal: Bytes) -> RpcResult<bool> {
		// `submit_seal` blocks on a full block import, so move it onto the
		// blocking pool and keep the RPC reactor free for other callers.
		let handle = self.handle.clone();
		tokio::task::spawn_blocking(move || handle.submit_seal(pre_hash, seal.0))
			.await
			.map_err(|err| {
				ErrorObjectOwned::owned(SUBMIT_FAILED, format!("submit task failed: {err}"), None::<()>)
			})
	}
}

fn no_task() -> ErrorObjectOwned {
	ErrorObjectOwned::owned(NO_TASK, "no mining task available yet", None::<()>)
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::sync::Mutex;

	/// Records every submission and accepts seals for a fixed set of live tasks.
	struct MockMiner {
		latest: Option<(H256, U256)>,
		live: Vec<H256>,
		submitted: Mutex<Vec<(H256, Vec<u8>)>>,
		changes: watch::Sender<u64>,
	}

	impl MockMiner {
		fn new(latest: Option<(H256, U256)>, live: Vec<H256>) -> Arc<Self> {
			Arc::new(Self {
				latest,
				live,
				submitted: Mutex::new(Vec::new()),
				changes: watch::channel(0).0,
			})
		}
	}

	impl ExternalMiner for MockMiner {
		fn current_task(&self) -> Option<(H256, U256)> {
			self.latest
		}

		fn submit_seal(&self, pre_hash: H256, seal: Vec<u8>) -> bool {
			self.submitted.lock().unwrap().push((pre_hash, seal));
			self.live.contains(&pre_hash)
		}

		fn task_changes(&self) -> watch::Receiver<u64> {
			self.changes.subscribe()
		}
	}

	fn mining(mock: Arc<MockMiner>) -> Mining {
		Mining::new(mock, "5Miner".into(), "proto-v1".into())
	}

	#[test]
	fn get_task_exposes_full_inputs() {
		let pre_hash = H256::from_low_u64_be(1);
		let mock = MockMiner::new(Some((pre_hash, U256::from(7))), vec![pre_hash]);
		let task = mining(mock).get_task().expect("task is available");
		assert_eq!(task.pre_hash, pre_hash);
		assert_eq!(task.difficulty, U256::from(7));
		assert_eq!(task.miner, "5Miner");
		assert_eq!(task.protocol, "proto-v1");
	}

	#[test]
	fn get_task_without_build_errors() {
		let err = mining(MockMiner::new(None, vec![])).get_task().unwrap_err();
		assert_eq!(err.code(), NO_TASK);
	}

	#[tokio::test]
	async fn submit_seal_imports_live_task() {
		let pre_hash = H256::from_low_u64_be(2);
		let mock = MockMiner::new(Some((pre_hash, U256::from(1))), vec![pre_hash]);
		let imported = mining(mock.clone())
			.submit_seal(pre_hash, Bytes(vec![1, 2, 3]))
			.await
			.expect("submission runs");
		assert!(imported);
		assert_eq!(mock.submitted.lock().unwrap().as_slice(), &[(pre_hash, vec![1, 2, 3])]);
	}

	#[tokio::test]
	async fn submit_seal_accepts_old_same_head_task() {
		let old = H256::from_low_u64_be(2);
		let latest = H256::from_low_u64_be(3);
		// `latest` is the freshest task, yet a seal for the older `old` still
		// imports because both are live on the current head.
		let mock = MockMiner::new(Some((latest, U256::from(1))), vec![old, latest]);
		let imported = mining(mock.clone())
			.submit_seal(old, Bytes(vec![9]))
			.await
			.expect("submission runs");
		assert!(imported);
		assert_eq!(mock.submitted.lock().unwrap().as_slice(), &[(old, vec![9])]);
	}

	#[tokio::test]
	async fn submit_seal_rejects_pre_hash_past_the_head() {
		let latest = H256::from_low_u64_be(3);
		let mock = MockMiner::new(Some((latest, U256::from(1))), vec![latest]);
		let imported = mining(mock.clone())
			.submit_seal(H256::from_low_u64_be(9), Bytes(vec![1]))
			.await
			.expect("submission runs");
		assert!(!imported);
	}
}
