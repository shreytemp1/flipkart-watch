use chrono::{TimeZone, Utc};
use serde_json::json;

use fkwatch::models::{CheckResult, Product, RunReport};
use fkwatch::notify::{char_len, chunk_message, render_message, telegram_payload};

fn products() -> [Product; 2] {
    [
        Product::new(
            "Space Black",
            "/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395",
            "MOBHGZZ8GH2Z85MY",
        ),
        Product::new(
            "Velvet Red",
            "/oppo-find-x9-velvet-red-256-gb/p/itma96fe4cab1395",
            "MOBHG8Y56JNQGGTX",
        ),
    ]
}

fn make_report(results: Vec<CheckResult>) -> RunReport {
    RunReport {
        pincode: "412101".to_string(),
        address: "KK Marks Building A & B, Pimpri-Chinchwad, Maharashtra".to_string(),
        results,
        checked_at: Utc.with_ymd_and_hms(2026, 10, 28, 14, 15, 0).unwrap(),
    }
}

#[test]
fn not_available_message() {
    let products = products();
    let report = make_report(vec![
        CheckResult {
            in_stock: false,
            deliverable: false,
            price: Some(79999.0),
            ..CheckResult::new(products[0].clone())
        },
        CheckResult {
            in_stock: false,
            deliverable: false,
            price: Some(79999.0),
            ..CheckResult::new(products[1].clone())
        },
    ]);
    let text = render_message(&report, "OPPO Find X9 12+256");
    assert!(text.contains("NOT AVAILABLE at 412101"));
    assert!(text.contains("KK Marks Building A &amp; B"));
    assert!(text.contains("Space Black — out of stock — ₹79,999"));
    assert!(text.contains("19:45 IST"));
    assert!(!text.contains("Buy now"));
}

#[test]
fn available_message_includes_price_and_link() {
    let products = products();
    let report = make_report(vec![
        CheckResult {
            in_stock: true,
            deliverable: true,
            available: true,
            price: Some(79999.0),
            ..CheckResult::new(products[0].clone())
        },
        CheckResult {
            in_stock: false,
            deliverable: false,
            price: Some(79999.0),
            ..CheckResult::new(products[1].clone())
        },
    ]);
    let text = render_message(&report, "OPPO Find X9 12+256");
    assert!(text.contains("AVAILABLE at 412101"));
    assert!(text.contains("₹79,999"));
    assert!(text.contains(
        "href=\"https://www.flipkart.com/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395?pid=MOBHGZZ8GH2Z85MY\""
    ));
    assert!(text.contains("Not available:"));
    assert!(text.contains("Velvet Red — out of stock"));
}

#[test]
fn coupon_qualifying_listing_shows_coupon_amount() {
    let gated = Product::new(
        "X9s Lavender Sky",
        "/oppo-find-x9s-lavender-sky-256-gb/p/itmeca45f29bf010",
        "MOBHN7ZW6SBDHZWR",
    )
    .with_min_coupon(10000.0);
    let report = make_report(vec![CheckResult {
        in_stock: true,
        deliverable: true,
        available: true,
        price: Some(84999.0),
        coupon: Some(10000.0),
        ..CheckResult::new(gated)
    }]);
    assert!(report.any_available());
    let text = render_message(&report, "OPPO Find X9 / Find X9s 256GB");
    assert!(text.contains("AVAILABLE at 412101"));
    assert!(text.contains("X9s Lavender Sky"));
    assert!(text.contains("coupon <b>₹10,000 off</b>"));
}

#[test]
fn gated_listing_without_coupon_is_not_announced_as_available() {
    let gated = Product::new(
        "X9s Lavender Sky",
        "/oppo-find-x9s-lavender-sky-256-gb/p/itmeca45f29bf010",
        "MOBHN7ZW6SBDHZWR",
    )
    .with_min_coupon(10000.0);
    let report = make_report(vec![CheckResult {
        in_stock: true,
        deliverable: true,
        available: true,
        price: Some(84999.0),
        coupon: Some(9000.0),
        ..CheckResult::new(gated)
    }]);
    assert!(!report.any_available());
    let text = render_message(&report, "OPPO Find X9 / Find X9s 256GB");
    assert!(text.contains("NOT AVAILABLE at 412101"));
    assert!(text.contains("no ₹10,000 coupon"));
}

#[test]
fn partial_errors_are_surfaced() {
    let products = products();
    let report = make_report(vec![
        CheckResult {
            in_stock: false,
            deliverable: false,
            price: Some(79999.0),
            ..CheckResult::new(products[0].clone())
        },
        CheckResult::with_error(products[1].clone(), "request failed after 3 attempts"),
    ]);
    let text = render_message(&report, "OPPO Find X9 12+256");
    assert!(text.contains("Could not check"));
    assert!(text.contains("request failed after 3 attempts"));
}

#[test]
fn all_errors_does_not_claim_unavailable() {
    let products = products();
    let report = make_report(vec![
        CheckResult::with_error(products[0].clone(), "NO_PINCODE"),
        CheckResult::with_error(products[1].clone(), "timeout"),
    ]);
    let text = render_message(&report, "OPPO Find X9 12+256");
    assert!(text.contains("COULD NOT CHECK"));
    assert!(!text.contains("NOT AVAILABLE"));
    assert!(text.contains("NO_PINCODE"));
}

#[test]
fn chunking_preserves_every_line() {
    let lines: Vec<String> = (0..100)
        .map(|index| format!("line {index} {}", "x".repeat(80)))
        .collect();
    let text = lines.join("\n");
    let chunks = chunk_message(&text, 500);
    assert!(chunks.len() > 1);
    assert!(chunks.iter().all(|chunk| char_len(chunk) <= 500));
    assert_eq!(chunks.join("\n").split('\n').collect::<Vec<_>>(), lines);
}

#[test]
fn telegram_payload_matches_expected_shape() {
    let payload = telegram_payload("42", "hello");
    assert_eq!(payload["chat_id"], json!("42"));
    assert_eq!(payload["text"], json!("hello"));
    assert_eq!(payload["parse_mode"], json!("HTML"));
    assert_eq!(payload["disable_web_page_preview"], json!(true));
}
