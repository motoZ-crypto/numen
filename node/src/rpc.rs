//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use std::{collections::BTreeMap, sync::Arc};

use jsonrpsee::RpcModule;

use sc_consensus_grandpa::{
	FinalityProofProvider, GrandpaJustificationStream, SharedAuthoritySet, SharedVoterState,
};
use sc_network::service::traits::NetworkService;
use sc_network_sync::SyncingService;
use sc_rpc::SubscriptionTaskExecutor;
use sc_transaction_pool_api::TransactionPool;

use sp_api::{CallApiAt, ProvideRuntimeApi};
use sp_block_builder::BlockBuilder;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_core::H256;
use sp_inherents::CreateInherentDataProviders;
use sp_runtime::traits::Block as BlockT;

use sc_client_api::{
	backend::{Backend, StorageProvider},
	client::BlockchainEvents,
	AuxStore, UsageProvider,
};

use fc_rpc::{EthBlockDataCacheTask, EthConfig};
use fc_rpc_core::types::{FeeHistoryCache, FeeHistoryCacheLimit, FilterPool};
use fc_storage::StorageOverride;
use fp_rpc::{ConvertTransaction, ConvertTransactionRuntimeApi, EthereumRuntimeRPCApi};

use solochain_template_runtime::{opaque::Block, AccountId, Balance, Nonce};

/// Dependencies for GRANDPA RPC.
pub struct GrandpaDeps<B> {
	/// Voting round info.
	pub shared_voter_state: SharedVoterState,
	/// Authority set info.
	pub shared_authority_set: SharedAuthoritySet<
		<Block as BlockT>::Hash,
		<<Block as BlockT>::Header as sp_runtime::traits::Header>::Number,
	>,
	/// Receives notifications about justification events from GRANDPA.
	pub justification_stream: GrandpaJustificationStream<Block>,
	/// Executor to drive the subscription manager in the GRANDPA RPC handler.
	pub subscription_executor: SubscriptionTaskExecutor,
	/// Finality proof provider.
	pub finality_provider: Arc<FinalityProofProvider<B, Block>>,
}

/// Extra dependencies for Ethereum-compatibility RPCs.
pub struct EthDeps<C, P, CT, CIDP> {
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
	/// Optional Substrate ↔ Ethereum transaction converter.
	pub converter: Option<CT>,
	/// Whether the local node is an authority/miner.
	pub is_authority: bool,
	/// Whether to enable the dev signer.
	pub enable_dev_signer: bool,
	/// Network service handle.
	pub network: Arc<dyn NetworkService>,
	/// Chain syncing service.
	pub sync: Arc<SyncingService<Block>>,
	/// Frontier backend (KV).
	pub frontier_backend: Arc<dyn fc_api::Backend<Block>>,
	/// Ethereum data access overrides.
	pub storage_override: Arc<dyn StorageOverride<Block>>,
	/// Cache for Ethereum block data.
	pub block_data_cache: Arc<EthBlockDataCacheTask<Block>>,
	/// `eth_*` filter pool.
	pub filter_pool: Option<FilterPool>,
	/// Fee history cache.
	pub fee_history_cache: FeeHistoryCache,
	/// Fee history cache size limit.
	pub fee_history_cache_limit: FeeHistoryCacheLimit,
	/// Maximum number of logs returned by a single query.
	pub max_past_logs: u32,
	/// Maximum block range allowed in `eth_getLogs`.
	pub max_block_range: u32,
	/// `eth_call` / `eth_estimateGas` gas-limit multiplier.
	pub execute_gas_limit_multiplier: u64,
	/// Allow RPC submission of unprotected legacy transactions.
	pub rpc_allow_unprotected_txs: bool,
	/// Mandated parent hashes (used by some L2 deployments).
	pub forced_parent_hashes: Option<BTreeMap<H256, H256>>,
	/// Inherent data provider factory used for pending state queries.
	pub pending_create_inherent_data_providers: CIDP,
}

/// Full client dependencies.
pub struct FullDeps<C, P, B, CT, CIDP> {
	/// The client instance to use.
	pub client: Arc<C>,
	/// Transaction pool instance.
	pub pool: Arc<P>,
	/// GRANDPA-specific dependencies.
	pub grandpa: GrandpaDeps<B>,
	/// Ethereum-compatibility dependencies.
	pub eth: EthDeps<C, P, CT, CIDP>,
}

/// Default `EthConfig` impl bound to the node's client/backend.
pub struct DefaultEthConfig<C, BE>(std::marker::PhantomData<(C, BE)>);

impl<C, BE> EthConfig<Block, C> for DefaultEthConfig<C, BE>
where
	C: StorageProvider<Block, BE> + Sync + Send + 'static,
	BE: Backend<Block> + 'static,
{
	type EstimateGasAdapter = ();
	type RuntimeStorageOverride =
		fc_rpc::frontier_backend_client::SystemAccountId32StorageOverride<Block, C, BE>;
}

/// Instantiate all full RPC extensions (Substrate + GRANDPA + Ethereum).
pub fn create_full<C, P, B, BE, CT, CIDP>(
	deps: FullDeps<C, P, B, CT, CIDP>,
	subscription_task_executor: SubscriptionTaskExecutor,
	pubsub_notification_sinks: Arc<
		fc_mapping_sync::EthereumBlockNotificationSinks<
			fc_mapping_sync::EthereumBlockNotification<Block>,
		>,
	>,
) -> Result<RpcModule<()>, Box<dyn std::error::Error + Send + Sync>>
where
	C: CallApiAt<Block> + ProvideRuntimeApi<Block>,
	C: HeaderBackend<Block> + HeaderMetadata<Block, Error = BlockChainError> + 'static,
	C: BlockchainEvents<Block> + AuxStore + UsageProvider<Block> + StorageProvider<Block, BE>,
	C: Send + Sync + 'static,
	C::Api: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
	C::Api: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
	C::Api: BlockBuilder<Block>,
	C::Api: ConvertTransactionRuntimeApi<Block>,
	C::Api: EthereumRuntimeRPCApi<Block>,
	P: TransactionPool<Block = Block, Hash = <Block as BlockT>::Hash> + 'static,
	B: sc_client_api::Backend<Block> + Send + Sync + 'static,
	BE: Backend<Block> + 'static,
	CT: ConvertTransaction<<Block as BlockT>::Extrinsic> + Send + Sync + 'static,
	CIDP: CreateInherentDataProviders<Block, ()> + Send + 'static,
{
	use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
	use sc_consensus_grandpa_rpc::{Grandpa, GrandpaApiServer};
	use substrate_frame_rpc_system::{System, SystemApiServer};

	use fc_rpc::{
		Eth, EthApiServer, EthDevSigner, EthFilter, EthFilterApiServer, EthPubSub,
		EthPubSubApiServer, EthSigner, LogsJournal, LogsJournalConfig, Net, NetApiServer, Web3,
		Web3ApiServer,
	};

	let mut module = RpcModule::new(());
	let FullDeps { client, pool, grandpa, eth } = deps;
	let GrandpaDeps {
		shared_voter_state,
		shared_authority_set,
		justification_stream,
		subscription_executor,
		finality_provider,
	} = grandpa;

	module.merge(System::new(client.clone(), pool.clone()).into_rpc())?;
	module.merge(TransactionPayment::new(client.clone()).into_rpc())?;
	module.merge(
		Grandpa::new(
			subscription_executor,
			shared_authority_set,
			shared_voter_state,
			justification_stream,
			finality_provider,
		)
		.into_rpc(),
	)?;

	let EthDeps {
		client: eth_client,
		pool: eth_pool,
		converter,
		is_authority,
		enable_dev_signer,
		network,
		sync,
		frontier_backend,
		storage_override,
		block_data_cache,
		filter_pool,
		fee_history_cache,
		fee_history_cache_limit,
		max_past_logs,
		max_block_range,
		execute_gas_limit_multiplier,
		rpc_allow_unprotected_txs,
		forced_parent_hashes,
		pending_create_inherent_data_providers,
	} = eth;

	let mut signers = Vec::<Box<dyn EthSigner>>::new();
	if enable_dev_signer {
		signers.push(Box::new(EthDevSigner::new()));
	}

	let logs_journal = Arc::new(LogsJournal::with_config(
		subscription_task_executor.clone(),
		storage_override.clone(),
		pubsub_notification_sinks.clone(),
		LogsJournalConfig::default(),
	));

	module.merge(
		Eth::<Block, C, P, CT, BE, CIDP, DefaultEthConfig<C, BE>>::new(
			eth_client.clone(),
			eth_pool.clone(),
			converter,
			sync.clone(),
			signers,
			storage_override.clone(),
			frontier_backend.clone(),
			is_authority,
			block_data_cache.clone(),
			fee_history_cache,
			fee_history_cache_limit,
			execute_gas_limit_multiplier,
			rpc_allow_unprotected_txs,
			forced_parent_hashes,
			pending_create_inherent_data_providers,
			// PoW chain: no consensus data provider needed for `pending` queries.
			None,
		)
		.replace_config::<DefaultEthConfig<C, BE>>()
		.into_rpc(),
	)?;

	if let Some(filter_pool) = filter_pool {
		module.merge(
			EthFilter::new(
				eth_client.clone(),
				frontier_backend.clone(),
				eth_pool.clone(),
				filter_pool,
				500_usize,
				max_past_logs,
				max_block_range,
				block_data_cache.clone(),
				logs_journal.clone(),
			)
			.into_rpc(),
		)?;
	}

	module.merge(
		EthPubSub::new(
			eth_pool,
			eth_client.clone(),
			sync,
			subscription_task_executor,
			storage_override,
			pubsub_notification_sinks,
			logs_journal,
		)
		.into_rpc(),
	)?;

	module.merge(Net::new(eth_client.clone(), network, true).into_rpc())?;
	module.merge(Web3::new(eth_client).into_rpc())?;

	Ok(module)
}
