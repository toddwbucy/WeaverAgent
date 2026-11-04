use super::*;

fn counts(owner: &OfferBudget, consumed: u32, reserved: usize) {
    assert_eq!(owner.counts(), Counts { consumed, reserved });
}

fn refusal(owner: &mut OfferBudget, cap: &str, consumed: u32, reserved: usize) {
    let refusal = owner.reserve().unwrap_err();
    let RefusalRecord::Instruction {
        template_id,
        case,
        detail,
    } = *refusal
    else {
        panic!("the offer limit requires an instruction refusal");
    };
    assert_eq!(template_id, "thesaurus-offer/1");
    assert_eq!(case, InstructionRefusal::InvalidContext);
    assert!(detail.contains(&format!("offer_pass_cap={cap}")));
    assert!(detail.contains(&format!("consumed={consumed}")));
    assert!(detail.contains(&format!("reserved={reserved}")));
    counts(owner, consumed, reserved);
}

#[test]
fn offer_budget_requires_an_elected_positive_cap_without_a_default() {
    refusal(&mut OfferBudget::new(None), "absent", 0, 0);
    refusal(&mut OfferBudget::new(Some(0)), "0", 0, 0);
    let mut owner = OfferBudget::new(Some(u32::MAX));
    let reservation = owner.reserve().unwrap();
    owner.admitted(&reservation).unwrap();
    assert_eq!(
        owner.recorded(&reservation),
        Ok(RecordedAllowance::Consumed)
    );
    counts(&owner, 1, 0);
}

#[test]
fn offer_budget_last_u32_allowance_cannot_overflow_the_count() {
    let mut owner = OfferBudget::new(Some(u32::MAX));
    // Seed only the boundary fixture, without performing four billion passes.
    owner.consumed = u32::MAX - 1;
    let last = owner.reserve().unwrap();
    refusal(&mut owner, "4294967295", u32::MAX - 1, 1);
    owner.admitted(&last).unwrap();
    owner.recorded(&last).unwrap();
    refusal(&mut owner, "4294967295", u32::MAX, 0);
}

#[test]
fn offer_budget_pending_writes_share_the_limit_with_consumed_passes() {
    let mut owner = OfferBudget::new(Some(2));
    let first = owner.reserve().unwrap();
    let second = owner.reserve().unwrap();
    refusal(&mut owner, "2", 0, 2);
    owner.admitted(&first).unwrap();
    refusal(&mut owner, "2", 0, 2);
    assert_eq!(owner.recorded(&first), Ok(RecordedAllowance::Consumed));
    refusal(&mut owner, "2", 1, 1);
    owner.admitted(&second).unwrap();
    assert_eq!(owner.recorded(&second), Ok(RecordedAllowance::Consumed));
    // A later match, retry or answer has no reset operation on this owner.
    refusal(&mut owner, "2", 2, 0);
}

#[test]
fn offer_budget_only_unadmitted_cancellation_returns_an_allowance() {
    let mut owner = OfferBudget::new(Some(1));
    let refused = owner.reserve().unwrap();
    assert_eq!(owner.cancel(&refused), Ok(Cancellation::Released));
    counts(&owner, 0, 0);
    let admitted = owner.reserve().unwrap();
    owner.admitted(&admitted).unwrap();
    assert_eq!(owner.cancel(&admitted), Ok(Cancellation::AwaitingRecord));
    assert_eq!(owner.cancel(&admitted), Ok(Cancellation::AwaitingRecord));
    refusal(&mut owner, "1", 0, 1);
    assert_eq!(
        owner.recorded(&admitted),
        Ok(RecordedAllowance::ConsumedAfterCancellation)
    );
    refusal(&mut owner, "1", 1, 0);
    assert_eq!(
        owner.cancel(&admitted),
        Err(AccountingError::UnknownReservation)
    );
    refusal(&mut owner, "1", 1, 0);
}

#[test]
fn offer_budget_cannot_charge_without_admission_or_charge_twice() {
    let mut owner = OfferBudget::new(Some(2));
    let reservation = owner.reserve().unwrap();
    assert_eq!(
        owner.recorded(&reservation),
        Err(AccountingError::NotAdmitted)
    );
    counts(&owner, 0, 1);
    owner.admitted(&reservation).unwrap();
    assert_eq!(
        owner.admitted(&reservation),
        Err(AccountingError::AlreadyAdmitted)
    );
    counts(&owner, 0, 1);
    assert_eq!(
        owner.recorded(&reservation),
        Ok(RecordedAllowance::Consumed)
    );
    for outcome in [
        owner.admitted(&reservation),
        owner.recorded(&reservation).map(|_| ()),
    ] {
        assert_eq!(outcome, Err(AccountingError::UnknownReservation));
    }
    counts(&owner, 1, 0);
    let replacement = owner.reserve().unwrap();
    assert_eq!(
        owner.recorded(&reservation),
        Err(AccountingError::UnknownReservation)
    );
    counts(&owner, 1, 1);
    owner.admitted(&replacement).unwrap();
    owner.recorded(&replacement).unwrap();
    refusal(&mut owner, "2", 2, 0);
}

#[test]
fn offer_budget_rejects_a_reservation_from_another_owner() {
    let mut first = OfferBudget::new(Some(1));
    let mut second = OfferBudget::new(Some(1));
    let foreign = first.reserve().unwrap();
    let local = second.reserve().unwrap();
    assert_eq!(
        second.admitted(&foreign),
        Err(AccountingError::UnknownReservation)
    );
    assert_eq!(
        second.cancel(&foreign),
        Err(AccountingError::UnknownReservation)
    );
    second.admitted(&local).unwrap();
    assert_eq!(
        second.recorded(&foreign),
        Err(AccountingError::UnknownReservation)
    );
    counts(&first, 0, 1);
    counts(&second, 0, 1);
    first.admitted(&foreign).unwrap();
    first.recorded(&foreign).unwrap();
    second.recorded(&local).unwrap();
    refusal(&mut first, "1", 1, 0);
    refusal(&mut second, "1", 1, 0);
}

#[test]
fn offer_budget_turn_cancellation_accounts_for_late_receipts_without_continuation() {
    let mut owner = OfferBudget::new(Some(4));
    let charged = owner.reserve().unwrap();
    owner.admitted(&charged).unwrap();
    owner.recorded(&charged).unwrap();
    let unadmitted = owner.reserve().unwrap();
    let admitted = owner.reserve().unwrap();
    owner.admitted(&admitted).unwrap();
    owner.cancel_turn();
    counts(&owner, 1, 1);
    refusal(&mut owner, "4", 1, 1);
    assert_eq!(
        owner.admitted(&unadmitted),
        Err(AccountingError::UnknownReservation)
    );
    assert_eq!(
        owner.recorded(&unadmitted),
        Err(AccountingError::UnknownReservation)
    );
    owner.cancel_turn();
    counts(&owner, 1, 1);
    assert_eq!(
        owner.recorded(&admitted),
        Ok(RecordedAllowance::ConsumedAfterCancellation)
    );
    counts(&owner, 2, 0);
    refusal(&mut owner, "4", 2, 0);
}
