//! # Prime
//!
//! A downgraded sudo privilege that only allows runtime upgrades and
//! killing proposals on the spender track.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

pub mod weights;
pub use weights::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use alloc::vec::Vec;
	use frame_support::{dispatch::DispatchClass, pallet_prelude::*};
	use frame_system::{pallet_prelude::*, WeightInfo as _};

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config<RuntimeEvent: From<Event<Self>>> {
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	/// Account allowed to upgrade the runtime and rotate this key.
	#[pallet::storage]
	pub type Key<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub key: Option<T::AccountId>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			if let Some(key) = &self.key {
				Key::<T>::put(key);
			}
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// The prime key moved to a new account.
		KeyChanged { old: T::AccountId, new: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The caller is not the prime key.
		RequirePrime,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Replace the runtime code, forwarding to `System::set_code` as root.
		#[pallet::call_index(0)]
		#[pallet::weight((
			T::WeightInfo::upgrade()
				.saturating_add(<T as frame_system::Config>::SystemWeightInfo::set_code()),
			DispatchClass::Operational,
		))]
		pub fn upgrade(origin: OriginFor<T>, code: Vec<u8>) -> DispatchResultWithPostInfo {
			Self::ensure_prime(origin)?;
			frame_system::Pallet::<T>::set_code(frame_system::RawOrigin::Root.into(), code)
		}

		/// Hand the prime key to a new account.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::set_key())]
		pub fn set_key(origin: OriginFor<T>, new: T::AccountId) -> DispatchResult {
			let old = Self::ensure_prime(origin)?;
			Key::<T>::put(&new);
			Self::deposit_event(Event::KeyChanged { old, new });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Require a signed origin matching the stored prime key.
		fn ensure_prime(origin: OriginFor<T>) -> Result<T::AccountId, DispatchError> {
			let who = ensure_signed(origin)?;
			ensure!(Key::<T>::get().as_ref() == Some(&who), Error::<T>::RequirePrime);
			Ok(who)
		}
	}
}
