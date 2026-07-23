use loyalty_core::{CoalitionRules, Merchant, MerchantId, Passport, TierSchedule};
use serde::Deserialize;

#[derive(Deserialize)]
struct Fixture {
    customer: String,
    merchant: String,
    earn_bps: u16,
    daily_cap: u64,
    max_receipt_units: u64,
    thresholds: Vec<u64>,
    amount_units: u64,
    expected_credit: u64,
    expected_remaining_after_redeem: u64,
}

#[test]
fn readme_demo_fixture_is_a_real_state_transition() {
    let fixture: Fixture = serde_json::from_str(include_str!("../fixtures/demo.json")).unwrap();
    let merchant = Merchant::new(
        MerchantId::new(fixture.merchant).unwrap(),
        fixture.earn_bps,
        fixture.daily_cap,
    )
    .unwrap();
    let rules = CoalitionRules::new(
        fixture.max_receipt_units,
        TierSchedule::new(fixture.thresholds).unwrap(),
    )
    .unwrap();
    let mut passport = Passport::new(fixture.customer).unwrap();

    let accrual = passport
        .accrue(&merchant, 1, fixture.amount_units, 1, &rules)
        .unwrap();
    assert_eq!(accrual.credit_units, fixture.expected_credit);
    let redemption = passport.redeem(merchant.id(), 10).unwrap();
    assert_eq!(
        redemption.remaining_units,
        fixture.expected_remaining_after_redeem
    );
}
