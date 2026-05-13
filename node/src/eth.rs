//! Frontier (Ethereum compatibility) node-side helpers.
//!
//! Hosts the Frontier KV mapping database and the background tasks that keep
//! it in sync with the Substrate canonical chain (mapping-sync, filter-pool
//! cleanup, fee-history cache).

use std::{
	collections::BTreeMap,
	path::PathBuf,
	sync::{Arc, Mutex},
	time::Duration,
};

use futures::{future, prelude::*};
use sc_client_api::BlockchainEvents;
use sc_network_sync::SyncingService;
use sc_service::{Configuration, TaskManager};

use fc_rpc::EthTask;
use fc_rpc_core::types::{FeeHistoryCache, FeeHistoryCacheLimit, FilterPool};
use fc_storage::StorageOverride;

use solochain_template_runtime::opaque::Block;

use crate::service::{FullBackend, FullClient};

/// Default fee-history cache capacity (in blocks).
pub const DEFAULT_FEE_HISTORY_LIMIT: u64 = 2048;
/// Default filter retention (in blocks) before being garbage-collected.
const FILTER_RETAIN_THRESHOLD: u64 = 100;

/// Frontier backend type alias for this node.
pub type FrontierBackend = fc_db::Backend<Block, FullClient>;

/// Returns the on-disk directory used for the Frontier KV database.
pub fn db_config_dir(config: &Configuration) -> PathBuf {
	config.base_path.config_dir(config.chain_spec.id())
}

/// Eagerly-built Frontier components shared between `new_partial`/`new_full`.
pub struct FrontierPartialComponents {
	/// Pool of active `eth_*` filters tracked by the node.
	pub filter_pool: Option<FilterPool>,
	/// LRU cache feeding `eth_feeHistory`.
	pub fee_history_cache: FeeHistoryCache,
	/// Maximum number of blocks retained in the fee-history cache.
	pub fee_history_cache_limit: FeeHistoryCacheLimit,
}

/// Build the in-memory Frontier components used by both partial and full setup.
pub fn new_frontier_partial() -> FrontierPartialComponents {
	FrontierPartialComponents {
		filter_pool: Some(Arc::new(Mutex::new(BTreeMap::new()))),
		fee_history_cache: Arc::new(Mutex::new(BTreeMap::new())),
		fee_history_cache_limit: DEFAULT_FEE_HISTORY_LIMIT,
	}
}

/// Spawn the background tasks required by the Frontier RPC layer.
#[allow(clippy::too_many_arguments)]
pub fn spawn_frontier_tasks(
	task_manager: &TaskManager,
	client: Arc<FullClient>,
	backend: Arc<FullBackend>,
	frontier_backend: Arc<FrontierBackend>,
	filter_pool: Option<FilterPool>,
	storage_override: Arc<dyn StorageOverride<Block>>,
	fee_history_cache: FeeHistoryCache,
	fee_history_cache_limit: FeeHistoryCacheLimit,
	sync: Arc<SyncingService<Block>>,
	pubsub_notification_sinks: Arc<
		fc_mapping_sync::EthereumBlockNotificationSinks<
			fc_mapping_sync::EthereumBlockNotification<Block>,
		>,
	>,
) {
	// Mapping-sync worker keeps the Frontier DB aligned with the Substrate chain.
	match &*frontier_backend {
		fc_db::Backend::KeyValue(kv) => {
			task_manager.spawn_essential_handle().spawn(
				"frontier-mapping-sync-worker",
				Some("frontier"),
				fc_mapping_sync::kv::MappingSyncWorker::new(
					client.import_notification_stream(),
					Duration::new(6, 0),
					client.clone(),
					backend,
					storage_override.clone(),
					kv.clone(),
					3,
					0u32,
					None,
					fc_mapping_sync::SyncStrategy::Normal,
					sync,
					pubsub_notification_sinks,
				)
				.for_each(|()| future::ready(())),
			);
		}
		fc_db::Backend::Sql(_) => {
			// SQL backend is not supported by this node.
		}
	}

	// Periodic GC for `eth_*` filters.
	if let Some(filter_pool) = filter_pool {
		task_manager.spawn_essential_handle().spawn(
			"frontier-filter-pool",
			Some("frontier"),
			EthTask::filter_pool_task(client.clone(), filter_pool, FILTER_RETAIN_THRESHOLD),
		);
	}

	// Fee-history cache maintenance.
	task_manager.spawn_essential_handle().spawn(
		"frontier-fee-history",
		Some("frontier"),
		EthTask::fee_history_task(
			client,
			storage_override,
			fee_history_cache,
			fee_history_cache_limit,
		),
	);
}
