use futures::{
	prelude::*,
	task::{Context, Poll},
};
use futures_timer::Delay;
use log::*;
use parking_lot::Mutex;
use sc_client_api::ImportNotifications;
use sc_consensus::{BlockImportParams, BoxBlockImport, StateAction, StorageChanges};
use sp_consensus::{BlockOrigin, Proposal};
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT},
	DigestItem,
};
use std::{
	collections::VecDeque,
	pin::Pin,
	sync::{
		atomic::{AtomicUsize, Ordering},
		Arc,
	},
	time::Duration,
};
use tokio::sync::watch;

use crate::{PowAlgorithm, Seal, LOG_TARGET, POW_ENGINE_ID};

/// Mining metadata. This is the information needed to start an actual mining loop.
#[derive(Clone, Eq, PartialEq)]
pub struct MiningMetadata<H, D> {
	/// Currently known best hash which the pre-hash is built on.
	pub best_hash: H,
	/// Mining pre-hash.
	pub pre_hash: H,
	/// Pre-runtime digest item.
	pub pre_runtime: Option<Vec<u8>>,
	/// Mining target difficulty.
	pub difficulty: D,
}

/// A build of mining, containing the metadata and the block proposal.
pub struct MiningBuild<Block: BlockT, Algorithm: PowAlgorithm<Block>> {
	/// Mining metadata.
	pub metadata: MiningMetadata<Block::Hash, Algorithm::Difficulty>,
	/// Mining proposal.
	pub proposal: Proposal<Block>,
}

/// Cap on builds retained for one head. A fresh task lands each tick, so without
/// a bound a stalled head would leak one full proposal per second. Past the cap
/// the oldest build is evicted; a miner still on it gets `false` from submit and
/// just pulls a newer task.
const MAX_LIVE_TASKS: usize = 256;

/// Version of the mining worker.
#[derive(Eq, PartialEq, Clone, Copy)]
pub struct Version(usize);

/// Mining worker that exposes structs to query the current mining build and submit mined blocks.
pub struct MiningHandle<
	Block: BlockT,
	Algorithm: PowAlgorithm<Block>,
	L: sc_consensus::JustificationSyncLink<Block>,
> {
	version: Arc<AtomicUsize>,
	task_changed: watch::Sender<u64>,
	algorithm: Arc<Algorithm>,
	justification_sync_link: Arc<L>,
	build: Arc<Mutex<VecDeque<MiningBuild<Block, Algorithm>>>>,
	block_import: Arc<Mutex<BoxBlockImport<Block>>>,
}

impl<Block, Algorithm, L> MiningHandle<Block, Algorithm, L>
where
	Block: BlockT,
	Algorithm: PowAlgorithm<Block>,
	Algorithm::Difficulty: 'static + Send,
	L: sc_consensus::JustificationSyncLink<Block>,
{
	fn increment_version(&self) {
		let version = self.version.fetch_add(1, Ordering::SeqCst) + 1;
		let _ = self.task_changed.send(version as u64);
	}

	pub(crate) fn new(
		algorithm: Algorithm,
		block_import: BoxBlockImport<Block>,
		justification_sync_link: L,
	) -> Self {
		Self {
			version: Arc::new(AtomicUsize::new(0)),
			task_changed: watch::channel(0).0,
			algorithm: Arc::new(algorithm),
			justification_sync_link: Arc::new(justification_sync_link),
			build: Arc::new(Mutex::new(VecDeque::new())),
			block_import: Arc::new(Mutex::new(block_import)),
		}
	}

	pub(crate) fn on_major_syncing(&self) {
		self.build.lock().clear();
		self.increment_version();
	}

	pub(crate) fn on_build(&self, value: MiningBuild<Block, Algorithm>) {
		let best_hash = value.metadata.best_hash;

		let mut builds = self.build.lock();
		if builds.back().map(|b| b.metadata.best_hash) != Some(best_hash) {
			builds.clear();
		}
		builds.push_back(value);
		while builds.len() > MAX_LIVE_TASKS {
			builds.pop_front();
		}
		let live = builds.len();
		drop(builds);

		debug!(target: LOG_TARGET, "New mining task on top of {}, {} live", best_hash, live);

		self.increment_version();
	}

	/// Get the version of the mining worker.
	///
	/// This returns type `Version` which can only compare equality. If `Version` is unchanged, then
	/// it can be certain that `best_hash` and `metadata` were not changed.
	pub fn version(&self) -> Version {
		Version(self.version.load(Ordering::SeqCst))
	}

	/// Subscribe to task changes so a caller can await the next task instead of
	/// polling.
	pub fn subscribe(&self) -> watch::Receiver<u64> {
		self.task_changed.subscribe()
	}

	/// Get the current best hash. `None` if the worker has just started or the client is doing
	/// major syncing.
	pub fn best_hash(&self) -> Option<Block::Hash> {
		self.build.lock().back().map(|b| b.metadata.best_hash)
	}

	/// Get a copy of the most recent mining metadata, if available.
	pub fn metadata(&self) -> Option<MiningMetadata<Block::Hash, Algorithm::Difficulty>> {
		self.build.lock().back().map(|b| b.metadata.clone())
	}

	/// Submit a seal found for `pre_hash`. The seal is validated again before
	/// import. Returns true on a successful import. A `pre_hash` the head has
	/// already moved past is no longer stored, so it returns false.
	#[allow(clippy::await_holding_lock)]
	pub async fn submit(&self, pre_hash: Block::Hash, seal: Seal) -> bool {
		let metadata = match self.build.lock().iter().find(|b| b.metadata.pre_hash == pre_hash) {
			Some(build) => build.metadata.clone(),
			None => {
				warn!(target: LOG_TARGET, "Unable to import mined block: no task for the submitted pre-hash",);
				return false;
			},
		};

		// Pre-check against the same realtime difficulty import recomputes.
		let difficulty = match self.algorithm.difficulty(metadata.best_hash) {
			Ok(difficulty) => difficulty,
			Err(err) => {
				warn!(target: LOG_TARGET, "Unable to import mined block: {}", err,);
				return false;
			},
		};

		match self.algorithm.verify(
			&BlockId::Hash(metadata.best_hash),
			&metadata.pre_hash,
			metadata.pre_runtime.as_ref().map(|v| &v[..]),
			&seal,
			difficulty,
		) {
			Ok(true) => (),
			Ok(false) => {
				warn!(target: LOG_TARGET, "Unable to import mined block: seal is invalid",);
				return false;
			},
			Err(err) => {
				warn!(target: LOG_TARGET, "Unable to import mined block: {}", err,);
				return false;
			},
		}

		let build = {
			let mut builds = self.build.lock();
			match builds.iter().position(|b| b.metadata.pre_hash == pre_hash) {
				Some(i) => builds.remove(i).expect("position is in bounds"),
				None => {
					warn!(target: LOG_TARGET, "Unable to import mined block: task already taken",);
					return false;
				},
			}
		};
		self.increment_version();

		let seal = DigestItem::Seal(POW_ENGINE_ID, seal);
		let (header, body) = build.proposal.block.deconstruct();

		let mut import_block = BlockImportParams::new(BlockOrigin::Own, header);
		import_block.post_digests.push(seal);
		import_block.body = Some(body);
		import_block.state_action =
			StateAction::ApplyChanges(StorageChanges::Changes(build.proposal.storage_changes));

		let header = import_block.post_header();
		let block_import = self.block_import.lock();

		match block_import.import_block(import_block).await {
			Ok(res) => {
				res.handle_justification(
					&header.hash(),
					*header.number(),
					&self.justification_sync_link,
				);

				// The block landed; drop every remaining build for the now-stale head.
				self.build.lock().clear();
				self.increment_version();

				info!(
					target: LOG_TARGET,
					"✅ Successfully mined block on top of: {}", build.metadata.best_hash
				);
				true
			},
			Err(err) => {
				warn!(target: LOG_TARGET, "Unable to import mined block: {}", err,);
				false
			},
		}
	}
}

impl<Block, Algorithm, L> Clone for MiningHandle<Block, Algorithm, L>
where
	Block: BlockT,
	Algorithm: PowAlgorithm<Block>,
	L: sc_consensus::JustificationSyncLink<Block>,
{
	fn clone(&self) -> Self {
		Self {
			version: self.version.clone(),
			task_changed: self.task_changed.clone(),
			algorithm: self.algorithm.clone(),
			justification_sync_link: self.justification_sync_link.clone(),
			build: self.build.clone(),
			block_import: self.block_import.clone(),
		}
	}
}

/// A stream that waits for a block import or timeout.
pub struct UntilImportedOrTimeout<Block: BlockT> {
	import_notifications: ImportNotifications<Block>,
	timeout: Duration,
	inner_delay: Option<Delay>,
}

impl<Block: BlockT> UntilImportedOrTimeout<Block> {
	/// Create a new stream using the given import notification and timeout duration.
	pub fn new(import_notifications: ImportNotifications<Block>, timeout: Duration) -> Self {
		Self { import_notifications, timeout, inner_delay: None }
	}
}

impl<Block: BlockT> Stream for UntilImportedOrTimeout<Block> {
	type Item = ();

	fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<()>> {
		let mut fire = false;

		loop {
			match Stream::poll_next(Pin::new(&mut self.import_notifications), cx) {
				Poll::Pending => break,
				Poll::Ready(Some(_)) => {
					fire = true;
				},
				Poll::Ready(None) => return Poll::Ready(None),
			}
		}

		let timeout = self.timeout;
		let inner_delay = self.inner_delay.get_or_insert_with(|| Delay::new(timeout));

		match Future::poll(Pin::new(inner_delay), cx) {
			Poll::Pending => (),
			Poll::Ready(()) => {
				fire = true;
			},
		}

		if fire {
			self.inner_delay = None;
			Poll::Ready(Some(()))
		} else {
			Poll::Pending
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{Error, PowAlgorithm};
	use futures::executor::block_on;
	use sc_consensus::{BlockCheckParams, BlockImport, ImportResult};
	use sp_core::{H256, U256};
	use sp_runtime::{
		testing::{Block as RawBlock, Header as TestHeader},
		OpaqueExtrinsic,
	};

	type Block = RawBlock<OpaqueExtrinsic>;

	/// Reports zero difficulty and accepts every seal, so `submit` outcomes
	/// depend only on the task queue and block import.
	struct AcceptAll;

	impl PowAlgorithm<Block> for AcceptAll {
		type Difficulty = U256;

		fn difficulty(&self, _parent: H256) -> Result<U256, Error<Block>> {
			Ok(U256::zero())
		}

		fn verify(
			&self,
			_parent: &BlockId<Block>,
			_pre_hash: &H256,
			_pre_digest: Option<&[u8]>,
			_seal: &Seal,
			_difficulty: U256,
		) -> Result<bool, Error<Block>> {
			Ok(true)
		}
	}

	/// Records every imported block and always succeeds.
	struct RecordingImport(Arc<Mutex<Vec<H256>>>);

	#[async_trait::async_trait]
	impl BlockImport<Block> for RecordingImport {
		type Error = sp_consensus::Error;

		async fn check_block(
			&self,
			_block: BlockCheckParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			Ok(ImportResult::Imported(Default::default()))
		}

		async fn import_block(
			&self,
			block: BlockImportParams<Block>,
		) -> Result<ImportResult, Self::Error> {
			self.0.lock().push(block.header.hash());
			Ok(ImportResult::Imported(Default::default()))
		}
	}

	#[allow(clippy::type_complexity)]
	fn handle() -> (MiningHandle<Block, AcceptAll, ()>, Arc<Mutex<Vec<H256>>>) {
		let imported = Arc::new(Mutex::new(Vec::new()));
		let import = RecordingImport(imported.clone());
		(MiningHandle::new(AcceptAll, Box::new(import), ()), imported)
	}

	fn build(head: u8, task: u64) -> MiningBuild<Block, AcceptAll> {
		MiningBuild {
			metadata: MiningMetadata {
				best_hash: H256::repeat_byte(head),
				pre_hash: H256::from_low_u64_be(task),
				pre_runtime: None,
				difficulty: U256::zero(),
			},
			proposal: Proposal {
				block: <Block as BlockT>::new(TestHeader::new_from_number(1), Vec::new()),
				storage_changes: Default::default(),
			},
		}
	}

	fn holds_task(handle: &MiningHandle<Block, AcceptAll, ()>, task: u64) -> bool {
		handle.build.lock().iter().any(|b| b.metadata.pre_hash == H256::from_low_u64_be(task))
	}

	#[test]
	fn on_build_evicts_the_oldest_past_the_cap() {
		let (handle, _) = handle();
		for task in 0..MAX_LIVE_TASKS as u64 {
			handle.on_build(build(0xAA, task));
		}
		assert_eq!(handle.build.lock().len(), MAX_LIVE_TASKS, "the cap itself still fits");
		assert!(holds_task(&handle, 0));

		handle.on_build(build(0xAA, MAX_LIVE_TASKS as u64));

		assert_eq!(
			handle.build.lock().len(),
			MAX_LIVE_TASKS,
			"one past the cap evicts instead of growing",
		);
		assert!(!holds_task(&handle, 0), "the oldest build is the one evicted");
		assert!(holds_task(&handle, 1));
		assert!(holds_task(&handle, MAX_LIVE_TASKS as u64));
	}

	#[test]
	fn on_build_drops_all_tasks_of_a_stale_head() {
		let (handle, _) = handle();
		for task in 0..3 {
			handle.on_build(build(0xAA, task));
		}

		handle.on_build(build(0xBB, 7));

		assert_eq!(handle.build.lock().len(), 1, "a new head starts an empty queue");
		assert!(holds_task(&handle, 7));
		assert_eq!(handle.best_hash(), Some(H256::repeat_byte(0xBB)));
	}

	#[test]
	fn on_major_syncing_clears_the_queue() {
		let (handle, _) = handle();
		for task in 0..3 {
			handle.on_build(build(0xAA, task));
		}

		handle.on_major_syncing();

		assert!(handle.build.lock().is_empty());
		assert_eq!(handle.best_hash(), None);
		assert!(handle.metadata().is_none());
	}

	#[test]
	fn submit_rejects_an_evicted_task() {
		let (handle, imported) = handle();
		for task in 0..=MAX_LIVE_TASKS as u64 {
			handle.on_build(build(0xAA, task));
		}

		assert!(!block_on(handle.submit(H256::from_low_u64_be(0), vec![1])));

		assert!(imported.lock().is_empty(), "an evicted task must not reach block import");
	}

	#[test]
	fn submit_imports_a_retained_older_task_and_clears_the_queue() {
		let (handle, imported) = handle();
		handle.on_build(build(0xAA, 1));
		handle.on_build(build(0xAA, 2));

		assert!(block_on(handle.submit(H256::from_low_u64_be(1), vec![1])));

		assert_eq!(imported.lock().len(), 1);
		assert!(
			handle.build.lock().is_empty(),
			"a landed block invalidates every remaining build",
		);
	}
}
