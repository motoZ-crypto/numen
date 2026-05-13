//! Frontier EVM configuration.
//!
//! Wires `pallet-evm`, `pallet-ethereum`, `pallet-base-fee` and
//! `pallet-evm-chain-id` into the runtime. Substrate `AccountId32` accounts
//! are mapped onto EVM `H160` addresses through Frontier's standard
//! `HashedAddressMapping<BlakeTwo256>`. PoW does not assign block authorship,
//! so `FindAuthor<H160>` always returns `None` (mirroring the
//! [`PowFindAuthor`](super::PowFindAuthor) used by `pallet-authorship`).

use core::marker::PhantomData;

use frame_support::{
	parameter_types,
	traits::{ConstU32, FindAuthor},
	weights::{constants::WEIGHT_REF_TIME_PER_MILLIS, Weight},
};
use pallet_ethereum::PostLogContent;
use pallet_evm::{
	EnsureAddressNever, EnsureAddressRoot, HashedAddressMapping, IsPrecompileResult, Precompile,
	PrecompileHandle, PrecompileResult, PrecompileSet,
};
use pallet_evm_precompile_bn128::{Bn128Add, Bn128Mul, Bn128Pairing};
use pallet_evm_precompile_modexp::Modexp;
use pallet_evm_precompile_simple::{ECRecover, Identity, Ripemd160, Sha256};
use sp_core::{H160, U256};
use sp_runtime::{
	traits::BlakeTwo256,
	ConsensusEngineId, Permill,
};

use super::NORMAL_DISPATCH_RATIO;
use crate::{Balances, Runtime, Timestamp};

/// Target block gas limit (matches the Frontier template default).
const BLOCK_GAS_LIMIT: u64 = 75_000_000;
/// Compute budget per block, in milliseconds, used to derive `WeightPerGas`.
///
/// PoW targets ~20s of wall-clock per block (see [`super::TargetBlockTime`]),
/// but the runtime caps actual on-chain compute to 2s of reference time
/// (see `RuntimeBlockWeights` in [`super`]). The EVM gas/weight conversion
/// must be calibrated against this real compute budget — not the wall-clock
/// block time — otherwise `WeightPerGas` is under-counted and a single
/// large-gas transaction can exhaust the block weight budget long before
/// reaching [`BLOCK_GAS_LIMIT`].
const WEIGHT_MILLIS_PER_BLOCK: u64 = 2_000;
/// Maximum PoV size (only relevant on parachains; kept for parity with the
/// Frontier template formula).
const MAX_POV_SIZE: u64 = 5 * 1024 * 1024;
/// Soft cap on storage growth per block, used to derive
/// `GasLimitStorageGrowthRatio`.
const MAX_STORAGE_GROWTH: u64 = 400 * 1024;

parameter_types! {
	/// EVM chain id. Set via genesis into `pallet-evm-chain-id` storage.
	pub const ChainId: u64 = 32026;
	pub BlockGasLimit: U256 = U256::from(BLOCK_GAS_LIMIT);
	pub TransactionGasLimit: Option<U256> = Some(fp_evm::MAX_TRANSACTION_GAS_LIMIT);
	pub const GasLimitPovSizeRatio: u64 = BLOCK_GAS_LIMIT.saturating_div(MAX_POV_SIZE);
	pub const GasLimitStorageGrowthRatio: u64 = BLOCK_GAS_LIMIT.saturating_div(MAX_STORAGE_GROWTH);
	pub WeightPerGas: Weight = Weight::from_parts(
		weight_per_gas(BLOCK_GAS_LIMIT, NORMAL_DISPATCH_RATIO, WEIGHT_MILLIS_PER_BLOCK),
		0,
	);
	pub PrecompilesValue: FrontierPrecompiles<Runtime> = FrontierPrecompiles::<_>::new();
}

/// Local copy of [`fp_evm::weight_per_gas`] used here in `const`-friendly form.
fn weight_per_gas(block_gas_limit: u64, txn_ratio: sp_runtime::Perbill, weight_ms: u64) -> u64 {
	let weight_per_block = WEIGHT_REF_TIME_PER_MILLIS.saturating_mul(weight_ms);
	let w = (txn_ratio * weight_per_block).saturating_div(block_gas_limit);
	core::cmp::max(w, 1)
}

/// PoW does not nominate a block author H160; EVM's `FindAuthor` returns the
/// zero address so opcodes like `COINBASE` evaluate deterministically.
pub struct EvmFindAuthorZero;
impl FindAuthor<H160> for EvmFindAuthorZero {
	fn find_author<'a, I>(_digests: I) -> Option<H160>
	where
		I: 'a + IntoIterator<Item = (ConsensusEngineId, &'a [u8])>,
	{
		Some(H160::zero())
	}
}

/// Precompile set covering the standard Ethereum precompiles 1-8.
///
/// The chain does not yet expose any chain-specific precompiles; addresses
/// 9 and above are unallocated. `ECRecover`, `SHA256`, `RIPEMD160` and
/// `Identity` use [`pallet_evm_precompile_simple`]; modexp uses
/// [`pallet_evm_precompile_modexp`]; the bn128 curve precompiles use
/// [`pallet_evm_precompile_bn128`].
pub struct FrontierPrecompiles<R>(PhantomData<R>);

impl<R> FrontierPrecompiles<R>
where
	R: pallet_evm::Config,
{
	pub fn new() -> Self {
		Self(PhantomData)
	}
	pub fn used_addresses() -> [H160; 8] {
		[
			hash(1),
			hash(2),
			hash(3),
			hash(4),
			hash(5),
			hash(6),
			hash(7),
			hash(8),
		]
	}
}

impl<R> Default for FrontierPrecompiles<R>
where
	R: pallet_evm::Config,
{
	fn default() -> Self {
		Self::new()
	}
}

impl<R> PrecompileSet for FrontierPrecompiles<R>
where
	R: pallet_evm::Config,
{
	fn execute(&self, handle: &mut impl PrecompileHandle) -> Option<PrecompileResult> {
		match handle.code_address() {
			a if a == hash(1) => Some(ECRecover::execute(handle)),
			a if a == hash(2) => Some(Sha256::execute(handle)),
			a if a == hash(3) => Some(Ripemd160::execute(handle)),
			a if a == hash(4) => Some(Identity::execute(handle)),
			a if a == hash(5) => Some(Modexp::execute(handle)),
			a if a == hash(6) => Some(Bn128Add::execute(handle)),
			a if a == hash(7) => Some(Bn128Mul::execute(handle)),
			a if a == hash(8) => Some(Bn128Pairing::execute(handle)),
			_ => None,
		}
	}

	fn is_precompile(&self, address: H160, _gas: u64) -> IsPrecompileResult {
		IsPrecompileResult::Answer {
			is_precompile: Self::used_addresses().contains(&address),
			extra_cost: 0,
		}
	}
}

fn hash(a: u64) -> H160 {
	H160::from_low_u64_be(a)
}

impl pallet_evm_chain_id::Config for Runtime {}

impl pallet_evm::Config for Runtime {
	type AccountProvider = pallet_evm::FrameSystemAccountProvider<Self>;
	type FeeCalculator = crate::BaseFee;
	type GasWeightMapping = pallet_evm::FixedGasWeightMapping<Self>;
	type WeightPerGas = WeightPerGas;
	// Use the Ethereum-side block hash mapping so that EVM `BLOCKHASH`
	// opcodes return the hashes recorded by `pallet-ethereum` for past
	// Ethereum-style blocks.
	type BlockHashMapping = pallet_ethereum::EthereumBlockHashMapping<Self>;
	// Substrate `AccountId32` cannot be safely lowered to a 20-byte address,
	// so the only direct EVM origins are root or `pallet-ethereum`'s own
	// self-contained calls (Step 4). All substrate-side users interact with
	// EVM contracts via the Ethereum extrinsic.
	type CallOrigin = EnsureAddressRoot<Self::AccountId>;
	type CreateOriginFilter = ();
	type CreateInnerOriginFilter = ();
	type WithdrawOrigin = EnsureAddressNever<Self::AccountId>;
	type AddressMapping = HashedAddressMapping<BlakeTwo256>;
	type Currency = Balances;
	type PrecompilesType = FrontierPrecompiles<Self>;
	type PrecompilesValue = PrecompilesValue;
	type ChainId = crate::EVMChainId;
	type BlockGasLimit = BlockGasLimit;
	type TransactionGasLimit = TransactionGasLimit;
	type Runner = pallet_evm::runner::stack::Runner<Self>;
	type OnChargeTransaction = ();
	type OnCreate = ();
	type FindAuthor = EvmFindAuthorZero;
	type GasLimitPovSizeRatio = GasLimitPovSizeRatio;
	type GasLimitStorageGrowthRatio = GasLimitStorageGrowthRatio;
	type Timestamp = Timestamp;
	type WeightInfo = pallet_evm::weights::SubstrateWeight<Self>;
}

parameter_types! {
	pub const PostBlockAndTxnHashes: PostLogContent = PostLogContent::BlockAndTxnHashes;
	pub const AllowUnprotectedTxs: bool = false;
}

impl pallet_ethereum::Config for Runtime {
	type StateRoot = pallet_ethereum::IntermediateStateRoot<<Runtime as frame_system::Config>::Version>;
	type PostLogContent = PostBlockAndTxnHashes;
	type ExtraDataLength = ConstU32<30>;
	type AllowUnprotectedTxs = AllowUnprotectedTxs;
}

parameter_types! {
	/// 1 gwei initial base fee.
	pub DefaultBaseFeePerGas: U256 = U256::from(1_000_000_000u64);
	pub DefaultElasticity: Permill = Permill::from_parts(125_000);
}

pub struct BaseFeeThreshold;
impl pallet_base_fee::BaseFeeThreshold for BaseFeeThreshold {
	fn lower() -> Permill {
		Permill::zero()
	}
	fn ideal() -> Permill {
		Permill::from_parts(500_000)
	}
	fn upper() -> Permill {
		Permill::from_parts(1_000_000)
	}
}

impl pallet_base_fee::Config for Runtime {
	type Threshold = BaseFeeThreshold;
	type DefaultBaseFeePerGas = DefaultBaseFeePerGas;
	type DefaultElasticity = DefaultElasticity;
}
