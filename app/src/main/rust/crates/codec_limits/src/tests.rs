use super::*;

#[test]
fn cumulative_allocations_are_bounded_unwind_safe_and_default_to_unlimited() {
    assert!(charge_allocation(usize::MAX).is_ok());
    let budget = AllocationBudget::new(10);
    budget.run(|| {
        charge_allocation(6).unwrap();
        charge_allocation(4).unwrap();
        assert!(charge_allocation(1).is_err());
        assert!(charge_allocation(usize::MAX).is_err());
    });
    with_decoder_limits(0, 0, || assert!(charge_allocation(1).is_err()));
    assert!(charge_allocation(1).is_ok());
}

#[test]
fn allocation_credit_is_owned_until_dictionary_drop() {
    let budget = AllocationBudget::new(100);
    let ticket = budget.run(|| reserve_dictionary(60).unwrap());
    budget.run(|| {
        assert!(reserve_dictionary(41).is_err());
        let second = reserve_dictionary(40).unwrap();
        assert!(reserve_dictionary(1).is_err());
        drop(second);
        assert!(reserve_dictionary(40).is_ok());
        AllocationBudget::new(1000).run(|| assert!(reserve_dictionary(41).is_err()));
    });
    drop(ticket);
    budget.run(|| assert!(reserve_dictionary(100).is_ok()));
    assert!(reserve_dictionary(usize::MAX).is_ok());
}

#[test]
fn allocation_scope_restores_after_panic_and_respects_dictionary_limit() {
    let budget = AllocationBudget::new(1);
    let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        budget.run(|| panic!("test"))
    }));
    assert!(panic.is_err());
    assert!(reserve_dictionary(usize::MAX).is_ok());
    with_decoder_limits(0, 0, || assert!(reserve_dictionary(1).is_err()));
}

#[test]
fn defaults_preserve_upstream_behavior() {
    assert!(check_dictionary(usize::MAX).is_ok());
    assert!(check_index(u64::MAX).is_ok());
}

#[test]
fn boundaries_are_inclusive_and_fail_before_allocation() {
    with_decoder_limits(1024, 3, || {
        assert!(check_dictionary(1024).is_ok());
        assert_eq!(
            check_dictionary(1025).unwrap_err().kind(),
            std::io::ErrorKind::OutOfMemory
        );
        assert!(check_index(3).is_ok());
        assert_eq!(
            check_index(4).unwrap_err().kind(),
            std::io::ErrorKind::OutOfMemory
        );
    });
    assert!(check_dictionary(1025).is_ok());
    assert!(check_index(4).is_ok());
}

#[test]
fn nested_scopes_cannot_relax_parent_policy() {
    with_decoder_limits(1024, 3, || {
        with_decoder_limits(2048, 6, || {
            assert!(check_dictionary(1025).is_err());
            assert!(check_index(4).is_err());
        });
        with_decoder_limits(0, 0, || {
            assert!(check_dictionary(1).is_err());
            assert!(check_index(1).is_err());
        });
        assert!(check_dictionary(1).is_ok());
        assert!(check_index(1).is_ok());
    });
}

#[test]
fn panic_restores_previous_thread_policy() {
    with_decoder_limits(1024, 3, || {
        let panic = std::panic::catch_unwind(|| with_decoder_limits(0, 0, || panic!("test")));
        assert!(panic.is_err());
        assert!(check_dictionary(1024).is_ok());
        assert!(check_index(3).is_ok());
    });
    assert!(check_dictionary(usize::MAX).is_ok());
}

#[test]
fn independent_threads_do_not_share_limits() {
    with_decoder_limits(0, 0, || {
        std::thread::spawn(|| {
            assert!(check_dictionary(usize::MAX).is_ok());
            assert!(check_index(u64::MAX).is_ok());
        })
        .join()
        .unwrap();
        assert!(check_dictionary(1).is_err());
    });
}
