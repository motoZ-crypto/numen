use crate::{
    mock::*, ActiveValidators, Error, Event, KickReason, LockInfo, OfflineSessionCount,
    OfflineThisSession, PendingValidators, RejoinCooldown, ValidatorLocks, ValidatorStatus,
};
use frame_support::{assert_noop, assert_ok, traits::Get};
use sp_runtime::{traits::Dispatchable, DispatchError, TokenError};

// region: lock

#[test]
fn lock_succeeds_and_records_state() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let lock_duration: u64 = <Test as crate::Config>::LockDuration::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));

        let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock recorded");
        assert_eq!(
            lock,
            LockInfo {
                amount: lock_amount,
                lock_block: 1,
                expiry_block: 1 + lock_duration,
                status: ValidatorStatus::Active,
            }
        );
        assert_eq!(PendingValidators::<Test>::get().to_vec(), vec![ALICE]);
        System::assert_last_event(
			Event::ValidatorLocked { who: ALICE, amount: lock_amount, expiry_block: 1 + lock_duration }.into(),
        );
    });
}

#[test]
fn lock_fails_without_session_keys() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        MissingSessionKeys::mutate(|set| {
            set.insert(ALICE);
        });
        assert_noop!(
            Validator::lock(RuntimeOrigin::signed(ALICE)),
            Error::<Test>::SessionKeysNotRegistered,
        );
        assert!(ValidatorLocks::<Test>::get(ALICE).is_none());
        assert!(PendingValidators::<Test>::get().is_empty());
    });
}

#[test]
fn lock_fails_when_balance_insufficient() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount - 1)]).execute_with(|| {
        assert_noop!(
            Validator::lock(RuntimeOrigin::signed(ALICE)),
            Error::<Test>::InsufficientBalance
        );
        assert!(ValidatorLocks::<Test>::get(ALICE).is_none());
        assert!(PendingValidators::<Test>::get().is_empty());
    });
}

#[test]
fn lock_fails_when_already_validator() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_noop!(
            Validator::lock(RuntimeOrigin::signed(ALICE)),
            Error::<Test>::AlreadyValidator
        );
    });
}

#[test]
fn lock_rejected_during_rejoin_cooldown() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let rejoin_cooldown: u64 = <Test as crate::Config>::RejoinCooldownPeriod::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        RejoinCooldown::<Test>::insert(ALICE, rejoin_cooldown);
        System::set_block_number(rejoin_cooldown);
        assert_noop!(
            Validator::lock(RuntimeOrigin::signed(ALICE)),
            Error::<Test>::InCooldown
        );
        assert!(RejoinCooldown::<Test>::get(ALICE).is_some());
    });
}

#[test]
fn lock_succeeds_after_cooldown_expires_and_clears_record() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let rejoin_cooldown: u64 = <Test as crate::Config>::RejoinCooldownPeriod::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        RejoinCooldown::<Test>::insert(ALICE, rejoin_cooldown);
        System::set_block_number(rejoin_cooldown + 1);
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert!(RejoinCooldown::<Test>::get(ALICE).is_none());
    });
}

#[test]
fn lock_fails_when_pending_queue_full() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let max_validators: u32 = <Test as crate::Config>::MaxValidators::get();
    let newcomer = (max_validators + 1) as AccountId;

    let validators: Vec<_> =
        (1..=max_validators as AccountId)
        .map(|i| (i, lock_amount))
        .collect();

    new_test_ext(validators.clone()).execute_with(|| {
        for (validator, _) in validators {
            assert_ok!(Validator::lock(RuntimeOrigin::signed(validator)));
        }
        assert_noop!(
            Validator::lock(RuntimeOrigin::signed(newcomer)),
            Error::<Test>::TooManyValidators
        );
        assert!(ValidatorLocks::<Test>::get(newcomer).is_none());
    });
}

#[test]
fn lock_rejected_while_pending() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert!(PendingValidators::<Test>::get().contains(&ALICE));
        assert_noop!(
            Validator::lock(RuntimeOrigin::signed(ALICE)),
            Error::<Test>::AlreadyValidator,
        );
    });
}

#[test]
fn lock_rejected_while_active() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        new_session(1);
        assert!(ActiveValidators::<Test>::get().contains(&ALICE));
        assert!(!PendingValidators::<Test>::get().contains(&ALICE));
        assert_noop!(
            Validator::lock(RuntimeOrigin::signed(ALICE)),
            Error::<Test>::AlreadyValidator,
        );
    });
}

#[test]
fn relock_after_request_exit_refreshes_lock_and_status() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        // Initial lock at block 1.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        new_session(1);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![ALICE]);
        let initial = ValidatorLocks::<Test>::get(ALICE).expect("locked");
        let initial_expiry = initial.expiry_block;

        // Voluntary exit; lock entry stays but status flips.
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));
        assert_eq!(
            ValidatorLocks::<Test>::get(ALICE).unwrap().status,
            ValidatorStatus::ExitRequested,
        );

        // Next session removes ALICE from the active set (our own
        // `ActiveValidators` storage reflects the truth).
        new_session(2);
        assert!(ActiveValidators::<Test>::get().is_empty());
        assert!(PendingValidators::<Test>::get().is_empty());
        // The currency lock is still in place; `ValidatorLocks` still exists.
        assert!(ValidatorLocks::<Test>::get(ALICE).is_some());

        // Advance a few blocks but do NOT let the lock expire.
        run_to_block(initial_expiry - 1);
        assert!(ValidatorLocks::<Test>::get(ALICE).is_some());

        // Re-lock now that ALICE is neither active nor pending. The new
        // lock overwrites the old one: status flips back to Active and
        // `expiry_block` is refreshed to `now + LockDuration`.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        let refreshed = ValidatorLocks::<Test>::get(ALICE).expect("relocked");
        assert_eq!(refreshed.status, ValidatorStatus::Active);
        assert!(refreshed.expiry_block > initial_expiry);
        assert_eq!(refreshed.lock_block, System::block_number());
        // ALICE is back in the pending queue ready for next promotion.
        assert!(PendingValidators::<Test>::get().contains(&ALICE));
    });
}

// endregion
// region: auto-renew

#[test]
fn auto_renew() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let lock_duration: u64 = <Test as crate::Config>::LockDuration::get();
    let renew_interval: u64 = <Test as crate::Config>::RenewInterval::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        // Lock at block 1 -> expiry = 11.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        let lock1 = ValidatorLocks::<Test>::get(ALICE).expect("lock recorded");
        // At block 6: expiry - now = 5, elapsed_window = 5 >= 5 -> renew.
        // New expiry = 6 + 10 = 16.
        let height = lock1.expiry_block - renew_interval;
        run_to_block(height);
        let lock2 = ValidatorLocks::<Test>::get(ALICE).expect("lock recorded");
        assert_eq!(lock2.expiry_block, height + lock_duration);
        System::assert_last_event(
			Event::ValidatorLocked { who: ALICE, amount: lock_amount, expiry_block: lock2.expiry_block }.into(),
        );
    });
}

#[test]
fn auto_renew_skips_within_interval() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let renew_interval: u64 = <Test as crate::Config>::RenewInterval::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        // Lock at block 1 -> expiry = 11.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        let lock1 = ValidatorLocks::<Test>::get(ALICE).expect("lock recorded");
        // At block 5: expiry - now = 6, elapsed_window = 5, not > 5 -> no renewal.
        run_to_block(lock1.expiry_block - renew_interval - 1);
        let lock2 = ValidatorLocks::<Test>::get(ALICE).expect("lock recorded");
        assert_eq!(lock2.expiry_block, lock1.expiry_block);
    });
}

fn auto_renew_skips_status(status: ValidatorStatus) {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let renew_interval: u64 = <Test as crate::Config>::RenewInterval::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        let lock1 = ValidatorLocks::<Test>::get(ALICE).expect("lock recorded");

        // Force the status to ExitRequested via storage so this test only
        // exercises the auto-renewal skip logic, not request_exit's own path.
        ValidatorLocks::<Test>::mutate(ALICE, |maybe| {
            maybe.as_mut().unwrap().status = status;
        });

        let height = lock1.expiry_block - renew_interval;
        run_to_block(height);

        let lock2 = ValidatorLocks::<Test>::get(ALICE).expect("lock retained");
        assert_eq!(lock2.expiry_block, lock1.expiry_block);
        assert_eq!(lock2.status, status);
    });
}

#[test]
fn auto_renew_skips_exit_status() {
    auto_renew_skips_status(ValidatorStatus::ExitRequested);
}

#[test]
fn auto_renew_skips_kicked_status() {
    auto_renew_skips_status(ValidatorStatus::Kicked);
}

// endregion
// region: request exit

#[test]
fn request_exit_changes_status_and_removes_from_pending() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount), (BOB, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        assert_eq!(PendingValidators::<Test>::get().to_vec(), vec![ALICE, BOB]);

        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));

        let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock kept");
        assert_eq!(lock.status, ValidatorStatus::ExitRequested);
        assert_eq!(PendingValidators::<Test>::get().to_vec(), vec![BOB]);
        System::assert_last_event(Event::ValidatorExitRequested { who: ALICE }.into());
    });
}

#[test]
fn request_exit_fails_when_not_validator() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        assert_noop!(
            Validator::request_exit(RuntimeOrigin::signed(ALICE)),
            Error::<Test>::NotValidator
        );
    });
}

#[test]
fn request_exit_fails_when_not_active() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let lock_duration: u64 = <Test as crate::Config>::LockDuration::get();

    new_test_ext(vec![(ALICE, lock_amount), (BOB, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));
        // Second call: status is ExitRequested, not Active.
        assert_noop!(
            Validator::request_exit(RuntimeOrigin::signed(ALICE)),
            Error::<Test>::InvalidStatus
        );

        // Kicked status also rejected.
        ValidatorLocks::<Test>::mutate(BOB, |maybe| {
            *maybe = Some(LockInfo {
                amount: lock_amount,
                lock_block: 1,
                expiry_block: 1 + lock_duration,
                status: ValidatorStatus::Kicked,
            });
        });
        assert_noop!(
            Validator::request_exit(RuntimeOrigin::signed(BOB)),
            Error::<Test>::InvalidStatus
        );
    });
}

#[test]
fn request_exit_keeps_lock_until_expiry() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));

        // Lock is still enforced: cannot move balance below the locked amount.
        let call = pallet_balances::Call::<Test>::transfer_keep_alive {
            dest: BOB,
            value: lock_amount,
        };
        let res = RuntimeCall::Balances(call).dispatch(RuntimeOrigin::signed(ALICE));
		assert_eq!(res.unwrap_err().error, DispatchError::Token(TokenError::Frozen));
    });
}

// endregion
// region: after exit

#[test]
fn lock_released_when_expiry_reached() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let lock_duration: u64 = <Test as crate::Config>::LockDuration::get();
    let existential_deposit: Balance = <Test as pallet_balances::Config>::ExistentialDeposit::get();
    
    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));

        run_to_block(1 + lock_duration);
        assert!(ValidatorLocks::<Test>::get(ALICE).is_none());
        System::assert_last_event(
			Event::LockReleased { who: ALICE, amount: lock_amount }.into(),
        );

        // Funds are fully transferable again.
        assert_ok!(Balances::transfer_keep_alive(
            RuntimeOrigin::signed(ALICE),
            BOB,
            lock_amount - existential_deposit
        ));
    });
}

#[test]
fn unexpired_locks_not_released() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let lock_duration: u64 = <Test as crate::Config>::LockDuration::get();

    new_test_ext(vec![(ALICE, lock_amount), (BOB, lock_amount)]).execute_with(|| {
        // Two validators locked at block 1; both expire at block 11.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        // Only ALICE requests exit so BOB keeps renewing.
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));

        // Advance to block 11: ALICE expires and is released. BOB renews twice
        // (at blocks 6 and 11), so its expiry advances to 21 and stays Active.
        run_to_block(1 + lock_duration);
        assert!(ValidatorLocks::<Test>::get(ALICE).is_none());
        let bob = ValidatorLocks::<Test>::get(BOB).expect("bob lock kept");
        assert_eq!(bob.expiry_block, 1 + lock_duration + lock_duration);
        assert_eq!(bob.status, ValidatorStatus::Active);
    });
}

#[test]
fn released_account_can_relock() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let lock_duration: u64 = <Test as crate::Config>::LockDuration::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));
        run_to_block(1 + lock_duration);
        assert!(ValidatorLocks::<Test>::get(ALICE).is_none());

        // Storage is cleaned up, so a fresh lock call must succeed.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        let lock = ValidatorLocks::<Test>::get(ALICE).expect("relock recorded");
        assert_eq!(lock.lock_block, System::block_number());
        assert_eq!(lock.expiry_block, System::block_number() + lock_duration);
        assert_eq!(lock.status, ValidatorStatus::Active);
    });
}

// endregion
// region: transfer

#[test]
fn transfer_within_unlocked_balance_succeeds() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let existential_deposit: Balance = <Test as pallet_balances::Config>::ExistentialDeposit::get();
    let unlocked_balance = 1;

    assert!(lock_amount >= existential_deposit, "lock amount must cover existential deposit for this test");

    new_test_ext(vec![(ALICE, lock_amount + unlocked_balance)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Balances::transfer_keep_alive(RuntimeOrigin::signed(ALICE), BOB, unlocked_balance));
    });
}

#[test]
fn transfer_one_above_unlocked_balance_fails() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let lock_duration: u64 = <Test as crate::Config>::LockDuration::get();
    let existential_deposit: Balance = <Test as pallet_balances::Config>::ExistentialDeposit::get();
    let unlocked_balance = 1;
    
    assert!(lock_amount >= existential_deposit, "lock amount must cover existential deposit for this test");

    new_test_ext(vec![(ALICE, lock_amount + unlocked_balance)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));

        // height #1
        let res = Balances::transfer_keep_alive(RuntimeOrigin::signed(ALICE), BOB, unlocked_balance + 1);
        assert_eq!(res.unwrap_err(), DispatchError::Token(TokenError::Frozen));

        // height #lock_duration
        run_to_block(lock_duration);
        let res = Balances::transfer_keep_alive(RuntimeOrigin::signed(ALICE), BOB, unlocked_balance + 1);
        assert_eq!(res.unwrap_err(), DispatchError::Token(TokenError::Frozen));
    });
}

#[test]
fn transfer_after_exit_succeeds() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let lock_duration: u64 = <Test as crate::Config>::LockDuration::get();
    let existential_deposit: Balance = <Test as pallet_balances::Config>::ExistentialDeposit::get();

    assert!(lock_amount >= existential_deposit, "lock amount must cover existential deposit for this test");

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));
        run_to_block(1 + lock_duration);
        assert_ok!(Balances::transfer_keep_alive(RuntimeOrigin::signed(ALICE), BOB, lock_amount - existential_deposit));
    });
}

// endregion
// region: session transition

#[test]
fn new_session_promotes_pending_validators() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount), (BOB, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));

        let set = new_session(1)
            .expect("set must change from empty");
        assert_eq!(set, vec![ALICE, BOB]);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![ALICE, BOB]);
        assert!(PendingValidators::<Test>::get().is_empty());
    });
}


#[test]
fn new_session_drops_pending_when_exists() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount), (BOB, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        new_session(1);

        PendingValidators::<Test>::mutate(|queue| {
            let _ = queue.try_push(ALICE);
        });
        new_session(2);

        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![ALICE, BOB]);
    });
}

#[test]
fn new_session_drops_pending_when_without_session_keys() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount), (BOB, lock_amount)]).execute_with(|| {
        // Both lock successfully (keys are present).
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        assert_eq!(PendingValidators::<Test>::get().to_vec(), vec![ALICE, BOB]);

        // Simulate ALICE purging her session keys between lock and the
        // next session boundary.
        MissingSessionKeys::mutate(|set| {
            set.insert(ALICE);
        });

        let next = new_session(1).expect("active set changed");
        assert_eq!(next, vec![BOB], "ALICE must be skipped, BOB promoted");
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![BOB]);
        // The pending queue is drained either way.
        assert!(PendingValidators::<Test>::get().is_empty());
    });
}

#[test]
fn new_session_drops_pending_when_capacity_full() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let max_validators: u32 = <Test as crate::Config>::MaxValidators::get();
    let newcomer = (max_validators + 1) as AccountId;

    let validators: Vec<_> =
        (1..=max_validators as AccountId)
        .map(|i| (i, lock_amount))
        .collect();

    new_test_ext(validators.clone()).execute_with(|| {
        for (validator, _) in validators {
            assert_ok!(Validator::lock(RuntimeOrigin::signed(validator)));
        }
        new_session(1);

        ValidatorLocks::<Test>::insert(
            newcomer,
            LockInfo {
                amount: lock_amount,
                lock_block: 1,
                expiry_block: 10,
                status: ValidatorStatus::Active,
            }
        );

        PendingValidators::<Test>::mutate(|queue| {
            let _ = queue.try_push(newcomer);
        });

        new_session(2);

        assert_eq!(ActiveValidators::<Test>::get().len(), max_validators as usize);
    });
}

#[test]
fn new_session_removes_exited_validator() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount), (BOB, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        new_session(1);

        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));

        let set = new_session(2)
            .expect("set must change after exit");
        assert_eq!(set, vec![BOB]);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![BOB]);
    });
}

#[test]
fn new_session_removes_kicked_validators() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount), (BOB, lock_amount), (CHARLIE, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        new_session(1);

        // Simulate kicks via direct storage mutation.
        ValidatorLocks::<Test>::mutate(BOB, |maybe| {
            maybe.as_mut().unwrap().status = ValidatorStatus::Kicked;
        });

        let set = new_session(2)
            .expect("set must change after kick");
        assert_eq!(set, vec![ALICE]);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![ALICE]);
    });
}

#[test]
fn new_session_returns_none_when_unchanged() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        new_session(1);
        // No new lock and no exit: next session must be a no-op.
        assert!(new_session(2).is_none());
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![ALICE]);
    });
}

#[test]
fn empty_set_fallback_keeps_previous_authorities_after_mass_kick() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount), (BOB, lock_amount)]).execute_with(|| {
        // Promote ALICE and BOB into the active set.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        let set = new_session(1).expect("initial promotion");
        assert_eq!(set, vec![ALICE, BOB]);

        // Kick both validators (e.g. equivocation). Their status switches to
        // `Kicked` so they are excluded from the next active set, leaving it
        // empty.
        Validator::note_equivocation(&ALICE);
        Validator::note_equivocation(&BOB);
        assert_eq!(
            ValidatorLocks::<Test>::get(ALICE).unwrap().status,
            ValidatorStatus::Kicked
        );
        assert_eq!(
            ValidatorLocks::<Test>::get(BOB).unwrap().status,
            ValidatorStatus::Kicked
        );

        // Empty-set fallback: returning `None` keeps `pallet-session`'s
        // authority set intact, while our own `ActiveValidators` storage is
        // updated to the empty truth.
        assert!(new_session(2).is_none());
        assert!(ActiveValidators::<Test>::get().is_empty());
    });
}

// endregion
// region: offline

#[test]
fn note_offline_ignores_non_validator() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        Validator::note_offline(&ALICE);
        assert!(OfflineThisSession::<Test>::get(ALICE).is_none());
    });
}

#[test]
fn consecutive_offline_kicks_validator() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let offline_threshold: u32 = <Test as crate::Config>::OfflineThreshold::get();

    new_test_ext(vec![(ALICE, lock_amount), (BOB, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        let _ = new_session(1);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![ALICE, BOB]);

        // ALICE misses three consecutive sessions while BOB stays online.
        for idx in 2..=offline_threshold+1 {
            Validator::note_offline(&ALICE);
            new_session(idx);
        }

        let alice_lock = ValidatorLocks::<Test>::get(ALICE).expect("lock retained");
        assert_eq!(alice_lock.status, ValidatorStatus::Kicked);
        assert!(RejoinCooldown::<Test>::get(ALICE).is_some());
        assert_eq!(OfflineSessionCount::<Test>::get(ALICE), 0);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![BOB]);
        // The transient set must be empty after processing.
        assert!(OfflineThisSession::<Test>::iter().next().is_none());
        // Event emitted with `Offline` reason.
        let kicked = System::events().into_iter().any(|e| matches!(
            e.event,
            RuntimeEvent::Validator(Event::ValidatorKicked { who: ALICE, reason: KickReason::Offline })
        ));
        assert!(kicked, "ValidatorKicked event with Offline reason expected");
    });
}

#[test]
fn intermittent_heartbeat_resets_counter() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let offline_threshold: u32 = <Test as crate::Config>::OfflineThreshold::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        new_session(1);

        for idx in 2..=offline_threshold {
            Validator::note_offline(&ALICE);
            new_session(idx);
        }
        assert_eq!(OfflineSessionCount::<Test>::get(ALICE), offline_threshold - 1);

        // Heartbeat received this session: counter resets.
        new_session(offline_threshold + 1);
        assert_eq!(OfflineSessionCount::<Test>::get(ALICE), 0);

        let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock retained");
        assert_eq!(lock.status, ValidatorStatus::Active);
        assert!(RejoinCooldown::<Test>::get(ALICE).is_none());
    });
}

#[test]
fn offline_does_not_overwrite_exit_requested() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount), (BOB, lock_amount)]).execute_with(|| {
        // Activate ALICE and BOB so the active set is populated.
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));
        new_session(1);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![ALICE, BOB]);

        // Pre-load offline counter to threshold-1 so the next offline report
        // would normally trigger a kick at the next session boundary.
        let threshold: u32 = <Test as crate::Config>::OfflineThreshold::get();
        OfflineSessionCount::<Test>::insert(ALICE, threshold - 1);

        // ALICE requests voluntary exit; she remains in `ActiveValidators`
        // until the next session boundary.
        assert_ok!(Validator::request_exit(RuntimeOrigin::signed(ALICE)));
        assert_eq!(
            ValidatorLocks::<Test>::get(ALICE).unwrap().status,
            ValidatorStatus::ExitRequested,
        );

        // ALICE is reported offline during the same session.
        Validator::note_offline(&ALICE);
        new_session(2);

        // The offline path must not downgrade ExitRequested into Kicked, must
        // not write a RejoinCooldown, and must clear stale offline counters.
        let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock retained");
        assert_eq!(lock.status, ValidatorStatus::ExitRequested);
        assert!(RejoinCooldown::<Test>::get(ALICE).is_none());
        assert_eq!(OfflineSessionCount::<Test>::get(ALICE), 0);
        // No `ValidatorKicked` event should have been emitted for ALICE.
        let kicked = System::events().into_iter().any(|e| matches!(
            e.event,
            RuntimeEvent::Validator(Event::ValidatorKicked { who: ALICE, .. })
        ));
        assert!(!kicked, "ExitRequested validator must not be kicked offline");
    });
}

// endregion
// region: equivocation

#[test]
fn note_equivocation_ignores_non_validator() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        Validator::note_equivocation(&ALICE);
        assert!(ValidatorLocks::<Test>::get(ALICE).is_none());
        assert!(RejoinCooldown::<Test>::get(ALICE).is_none());
    });
}

#[test]
fn note_equivocation_kicks_active_validator() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();
    let lock_duration: u64 = <Test as crate::Config>::LockDuration::get();
    let rejoin_cooldown: u64 = <Test as crate::Config>::RejoinCooldownPeriod::get();

    new_test_ext(vec![(ALICE, lock_amount), (BOB, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        assert_ok!(Validator::lock(RuntimeOrigin::signed(BOB)));

        Validator::note_equivocation(&ALICE);

        let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock retained");
        assert_eq!(lock.status, ValidatorStatus::Kicked);
        // Lock amount and expiry are unchanged so funds unlock at the original block.
        assert_eq!(lock.amount, lock_amount);
        assert_eq!(lock.expiry_block, System::block_number() + lock_duration);

        let cooldown = RejoinCooldown::<Test>::get(ALICE).expect("cooldown recorded");
        assert_eq!(cooldown, System::block_number() + rejoin_cooldown);

        System::assert_last_event(
            Event::ValidatorKicked { who: ALICE, reason: KickReason::Equivocation }.into(),
        );

        let set = new_session(1).expect("set must shrink");
        assert_eq!(set, vec![BOB]);
        assert_eq!(ActiveValidators::<Test>::get().to_vec(), vec![BOB]);

    });
}

#[test]
fn note_equivocation_is_idempotent() {
    let lock_amount: Balance = <Test as crate::Config>::LockAmount::get();

    new_test_ext(vec![(ALICE, lock_amount)]).execute_with(|| {
        assert_ok!(Validator::lock(RuntimeOrigin::signed(ALICE)));
        Validator::note_equivocation(&ALICE);
        let cooldown_first = RejoinCooldown::<Test>::get(ALICE).expect("cooldown recorded");

        // Advance a block and report again: the cooldown deadline must not move.
        System::set_block_number(2);
        Validator::note_equivocation(&ALICE);

        let cooldown_second = RejoinCooldown::<Test>::get(ALICE).expect("cooldown retained");
        assert_eq!(cooldown_first, cooldown_second);

        let lock = ValidatorLocks::<Test>::get(ALICE).expect("lock retained");
        assert_eq!(lock.status, ValidatorStatus::Kicked);
    });
}

// endregion
