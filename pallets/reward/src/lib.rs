//! Block reward pallet.
//!
//! Reads the PoW pre-runtime digest to identify the block author (miner),
//! then mints a configurable reward to their account on each block.
//!
//! Orphan and uncle blocks receive no reward because their state changes
//! are never applied to the canonical chain.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[frame_support::pallet]
pub mod pallet {
	use codec::Decode;
	use frame_support::{
		pallet_prelude::*,
		traits::Currency,
	};
	use frame_system::pallet_prelude::*;
	use sp_consensus_pow::POW_ENGINE_ID;

	type BalanceOf<T> =
		<<T as Config>::Currency as Currency<<T as frame_system::Config>::AccountId>>::Balance;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The currency used to mint block rewards.
		type Currency: Currency<Self::AccountId>;

		/// Fixed reward per block (in smallest units).
		#[pallet::constant]
		type BlockReward: Get<BalanceOf<Self>>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(_n: BlockNumberFor<T>) {
			if let Some(author) = Self::find_author() {
				let reward = T::BlockReward::get();
				if !reward.is_zero() {
					let _ = T::Currency::deposit_creating(&author, reward);
				}
			}
		}
	}

	impl<T: Config> Pallet<T> {
		/// Extract the block author from the PoW pre-runtime digest.
		///
		/// The miner encodes their `AccountId` as the payload of a
		/// `PreRuntime(POW_ENGINE_ID, _)` digest item.
		fn find_author() -> Option<T::AccountId> {
			let digest = frame_system::Pallet::<T>::digest();
			for log in digest.logs.iter() {
				if let sp_runtime::DigestItem::PreRuntime(engine, data) = log {
					if *engine == POW_ENGINE_ID {
						return T::AccountId::decode(&mut &data[..]).ok();
					}
				}
			}
			None
		}
	}
}
