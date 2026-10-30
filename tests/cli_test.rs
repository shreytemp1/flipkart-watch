use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};

use fkwatch::cli::should_notify;
use fkwatch::config::NotifyMode;
use fkwatch::models::{CheckResult, Product, RunReport};
use fkwatch::state::StateEntry;

fn product() -> Product {
    Product::new("Space Black", "/x", "ABC")
}

fn report(results: Vec<CheckResult>) -> RunReport {
    RunReport {
        pincode: "412101".to_string(),
        address: "Bengaluru".to_string(),
        results,
        checked_at: Utc.with_ymd_and_hms(2026, 10, 28, 14, 15, 0).unwrap(),
    }
}

#[test]
fn always_mode_always_notifies() {
    let report = report(vec![CheckResult::new(product())]);
    assert!(should_notify(NotifyMode::Always, &report, &BTreeMap::new()));
}

#[test]
fn available_mode_skips_when_nothing_available() {
    let report = report(vec![CheckResult::new(product())]);
    assert!(!should_notify(
        NotifyMode::Available,
        &report,
        &BTreeMap::new()
    ));
}

#[test]
fn available_mode_notifies_when_one_is_available() {
    let result = CheckResult {
        in_stock: true,
        deliverable: true,
        available: true,
        ..CheckResult::new(product())
    };
    assert!(should_notify(
        NotifyMode::Available,
        &report(vec![result]),
        &BTreeMap::new()
    ));
}

#[test]
fn available_mode_skips_when_every_check_failed() {
    let failed = CheckResult::with_error(product(), "NO_PINCODE");
    assert!(!should_notify(
        NotifyMode::Available,
        &report(vec![failed]),
        &BTreeMap::new()
    ));
}

#[test]
fn available_mode_respects_the_coupon_gate() {
    let gated = Product::new("X9s", "/x", "XYZ").with_min_coupon(10000.0);
    let no_coupon = CheckResult {
        in_stock: true,
        deliverable: true,
        available: true,
        ..CheckResult::new(gated.clone())
    };
    assert!(!should_notify(
        NotifyMode::Available,
        &report(vec![no_coupon]),
        &BTreeMap::new()
    ));

    let with_coupon = CheckResult {
        in_stock: true,
        deliverable: true,
        available: true,
        coupon: Some(10000.0),
        ..CheckResult::new(gated)
    };
    assert!(should_notify(
        NotifyMode::Available,
        &report(vec![with_coupon]),
        &BTreeMap::new()
    ));
}

#[test]
fn changes_mode_uses_previous_state() {
    let unchanged = CheckResult {
        price: Some(100.0),
        ..CheckResult::new(product())
    };
    let previous = BTreeMap::from([(
        product().pid,
        StateEntry {
            available: false,
            price: Some(100.0),
            error: None,
        },
    )]);
    assert!(!should_notify(
        NotifyMode::Changes,
        &report(vec![unchanged]),
        &previous
    ));

    let changed = CheckResult {
        price: Some(90.0),
        ..CheckResult::new(product())
    };
    assert!(should_notify(
        NotifyMode::Changes,
        &report(vec![changed]),
        &previous
    ));
}
