//! OpenGov configuration. Token holders steer treasury spends and bounty
//! approvals through referenda. The spender origin carries a funding cap and
//! never reaches runtime level calls, which stay on the root track.

use crate::{
	AccountId, Balance, Balances, BlockNumber, Preimage, Referenda, Runtime, RuntimeCall,
	RuntimeEvent, RuntimeOrigin, Scheduler, System, Treasury, DAYS, HOURS, UNIT,
};
use alloc::borrow::Cow;
use frame_support::{
	parameter_types,
	traits::{ConstU32, EitherOf},
};
use frame_system::{EnsureRoot, EnsureRootWithSuccess, EnsureSigned};
use pallet_referenda::{Curve, Track, TrackInfo};
use sp_runtime::{str_array as s, FixedI64};

pub use pallet_custom_origins::Spender;

#[frame_support::pallet]
pub mod pallet_custom_origins {
	use crate::Balance;
	use frame_support::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[derive(
		PartialEq, Eq, Clone, MaxEncodedLen, Encode, Decode, DecodeWithMemTracking, TypeInfo, Debug,
	)]
	#[pallet::origin]
	pub enum Origin {
		/// Origin able to spend treasury funds and approve bounties.
		Spender,
	}

	macro_rules! decl_ensure {
		(
			$vis:vis type $name:ident: EnsureOrigin<Success = $success_type:ty> {
				$( $item:ident = $success:expr, )*
			}
		) => {
			$vis struct $name;
			impl<O: OriginTrait + From<Origin>> EnsureOrigin<O> for $name
			where
				for<'a> &'a O::PalletsOrigin: TryInto<&'a Origin>,
			{
				type Success = $success_type;
				fn try_origin(o: O) -> Result<Self::Success, O> {
					match o.caller().try_into() {
						$(
							Ok(Origin::$item) => return Ok($success),
						)*
						_ => (),
					}
					Err(o)
				}
				#[cfg(feature = "runtime-benchmarks")]
				fn try_successful_origin() -> Result<O, ()> {
					let _result: Result<O, ()> = Err(());
					$(
						let _result: Result<O, ()> = Ok(O::from(Origin::$item));
					)*
					_result
				}
			}
		};
	}

	decl_ensure! {
		pub type Spender: EnsureOrigin<Success = Balance> {
			Spender = Balance::MAX,
		}
	}
}

const fn percent(x: i32) -> FixedI64 {
	FixedI64::from_rational(x as u128, 100)
}

const APP_ROOT: Curve = Curve::make_reciprocal(2, 14, percent(80), percent(50), percent(100));
const SUP_ROOT: Curve = Curve::make_linear(14, 14, percent(0), percent(50));
const APP_SPENDER: Curve = Curve::make_linear(14, 14, percent(50), percent(100));
const SUP_SPENDER: Curve = Curve::make_linear(14, 14, percent(0), percent(50));

const TRACKS_DATA: [Track<u16, Balance, BlockNumber>; 2] = [
	Track {
		id: 0,
		info: TrackInfo {
			name: s("root"),
			max_deciding: 1,
			decision_deposit: 10_000 * UNIT,
			prepare_period: HOURS,
			decision_period: 14 * DAYS,
			confirm_period: DAYS,
			min_enactment_period: DAYS,
			min_approval: APP_ROOT,
			min_support: SUP_ROOT,
		},
	},
	Track {
		id: 1,
		info: TrackInfo {
			name: s("spender"),
			max_deciding: 10,
			decision_deposit: 1_000 * UNIT,
			prepare_period: HOURS,
			decision_period: 7 * DAYS,
			confirm_period: DAYS,
			min_enactment_period: DAYS,
			min_approval: APP_SPENDER,
			min_support: SUP_SPENDER,
		},
	},
];

pub struct TracksInfo;
impl pallet_referenda::TracksInfo<Balance, BlockNumber> for TracksInfo {
	type Id = u16;
	type RuntimeOrigin = <RuntimeOrigin as frame_support::traits::OriginTrait>::PalletsOrigin;

	fn tracks() -> impl Iterator<Item = Cow<'static, Track<Self::Id, Balance, BlockNumber>>> {
		TRACKS_DATA.iter().map(Cow::Borrowed)
	}

	fn track_for(id: &Self::RuntimeOrigin) -> Result<Self::Id, ()> {
		if let Ok(system_origin) = frame_system::RawOrigin::try_from(id.clone()) {
			match system_origin {
				frame_system::RawOrigin::Root => Ok(0),
				_ => Err(()),
			}
		} else if let Ok(custom) = pallet_custom_origins::Origin::try_from(id.clone()) {
			match custom {
				pallet_custom_origins::Origin::Spender => Ok(1),
			}
		} else {
			Err(())
		}
	}
}

parameter_types! {
	pub const VoteLockingPeriod: BlockNumber = 7 * DAYS;
	pub const AlarmInterval: BlockNumber = 1;
	pub const SubmissionDeposit: Balance = 100 * UNIT;
	pub const UndecidingTimeout: BlockNumber = 14 * DAYS;
}

impl pallet_conviction_voting::Config for Runtime {
	type WeightInfo = pallet_conviction_voting::weights::SubstrateWeight<Runtime>;
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type VoteLockingPeriod = VoteLockingPeriod;
	type MaxVotes = ConstU32<512>;
	type MaxTurnout =
		frame_support::traits::tokens::currency::ActiveIssuanceOf<Balances, AccountId>;
	type Polls = Referenda;
	type BlockNumberProvider = System;
	type VotingHooks = ();
}

impl pallet_custom_origins::Config for Runtime {}

/// Treasury and bounty spends accept either root or the OpenGov spender track.
pub type TreasurySpender = EitherOf<EnsureRootWithSuccess<AccountId, super::MaxBalance>, Spender>;

impl pallet_referenda::Config for Runtime {
	type WeightInfo = pallet_referenda::weights::SubstrateWeight<Runtime>;
	type RuntimeCall = RuntimeCall;
	type RuntimeEvent = RuntimeEvent;
	type Scheduler = Scheduler;
	type Currency = Balances;
	type SubmitOrigin = EnsureSigned<AccountId>;
	type CancelOrigin = EnsureRoot<AccountId>;
	type KillOrigin = EnsureRoot<AccountId>;
	type Slash = Treasury;
	type Votes = pallet_conviction_voting::VotesOf<Runtime>;
	type Tally = pallet_conviction_voting::TallyOf<Runtime>;
	type SubmissionDeposit = SubmissionDeposit;
	type MaxQueued = ConstU32<100>;
	type UndecidingTimeout = UndecidingTimeout;
	type AlarmInterval = AlarmInterval;
	type Tracks = TracksInfo;
	type Preimages = Preimage;
	type BlockNumberProvider = System;
}
