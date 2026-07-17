//! Handwritten weights covering only this pallet's own work. The `upgrade`
//! call adds the system `set_code` weight on top at the call site.

use core::marker::PhantomData;
use frame_support::{traits::Get, weights::Weight};

pub trait WeightInfo {
	fn upgrade() -> Weight;
	fn set_key() -> Weight;
}

pub struct SubstrateWeight<T>(PhantomData<T>);
impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn upgrade() -> Weight {
		Weight::from_parts(9_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(1_u64))
	}
	fn set_key() -> Weight {
		Weight::from_parts(9_000_000, 0)
			.saturating_add(T::DbWeight::get().reads_writes(1_u64, 1_u64))
	}
}

impl WeightInfo for () {
	fn upgrade() -> Weight {
		Weight::zero()
	}
	fn set_key() -> Weight {
		Weight::zero()
	}
}
