use std::collections::BTreeMap;

use chrono::{TimeZone, Utc};

use fkwatch::models::{CheckResult, Product, RunReport};
use fkwatch::state::{is_changed, load_state, save_state, StateEntry};
use tempfile::tempdir;

fn product() -> Product {
    Product::new(
        "Space Black",
        "/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395",
        "MOBHGZZ8GH2Z85MY",
    )
}

fn report(results: Vec<CheckResult>) -> RunReport {
    RunReport {
        pincode: "412101".to_string(),
        address: "test".to_string(),
        results,
        checked_at: Utc.with_ymd_and_hms(2026, 10, 28, 14, 15, 0).unwrap(),
    }
}

fn previous(
    available: bool,
    price: Option<f64>,
    error: Option<&str>,
) -> BTreeMap<String, StateEntry> {
    BTreeMap::from([(
        product().pid,
        StateEntry {
            available,
            price,
            error: error.map(str::to_string),
        },
    )])
}

#[test]
fn first_run_is_always_a_change() {
    let report = report(vec![CheckResult {
        in_stock: false,
        price: Some(79999.0),
        ..CheckResult::new(product())
    }]);
    assert!(is_changed(&BTreeMap::new(), &report));
}

#[test]
fn availability_flip_is_a_change() {
    let report = report(vec![CheckResult {
        in_stock: true,
        deliverable: true,
        available: true,
        price: Some(79999.0),
        ..CheckResult::new(product())
    }]);
    assert!(is_changed(&previous(false, Some(79999.0), None), &report));
}

#[test]
fn same_status_and_price_is_not_a_change() {
    let report = report(vec![CheckResult {
        in_stock: false,
        price: Some(79999.0),
        ..CheckResult::new(product())
    }]);
    assert!(!is_changed(&previous(false, Some(79999.0), None), &report));
}

#[test]
fn price_change_is_a_change() {
    let report = report(vec![CheckResult {
        in_stock: false,
        price: Some(74999.0),
        ..CheckResult::new(product())
    }]);
    assert!(is_changed(&previous(false, Some(79999.0), None), &report));
}

#[test]
fn recovering_from_an_error_is_a_change() {
    let report = report(vec![CheckResult {
        in_stock: true,
        deliverable: true,
        available: true,
        price: Some(78978.0),
        ..CheckResult::new(product())
    }]);
    assert!(is_changed(
        &previous(false, None, Some("NO_PINCODE")),
        &report
    ));
}

#[test]
fn current_error_does_not_spam() {
    let report = report(vec![CheckResult::with_error(product(), "timeout")]);
    assert!(!is_changed(&previous(false, Some(79999.0), None), &report));
}

#[test]
fn state_round_trips_through_disk() {
    let dir = tempdir().unwrap();
    let path = dir.path().join(".state").join("fkwatch.json");
    let report = report(vec![CheckResult {
        in_stock: false,
        price: Some(79999.0),
        ..CheckResult::new(product())
    }]);
    save_state(&path, &report).unwrap();
    let loaded = load_state(&path);
    assert_eq!(loaded, previous(false, Some(79999.0), None));
}
