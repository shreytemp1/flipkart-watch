mod support;

use fkwatch::flipkart::{best_coupon, parse_page_json, FlipkartError};
use fkwatch::models::Product;
use serde_json::json;

fn product() -> Product {
    Product::new(
        "Space Black",
        "/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395",
        "MOBHGZZ8GH2Z85MY",
    )
}

fn x9s() -> Product {
    Product::new(
        "X9s Lavender Sky",
        "/oppo-find-x9s-lavender-sky-256-gb/p/itmeca45f29bf010",
        "MOBHN7ZW6SBDHZWR",
    )
    .with_min_coupon(10000.0)
}

#[test]
fn out_of_stock_is_not_available() {
    let result = parse_page_json(&support::fixture("out_of_stock.json"), &product()).unwrap();
    assert!(result.ok());
    assert!(!result.in_stock);
    assert!(!result.deliverable);
    assert!(!result.available);
    assert_eq!(result.price, Some(79999.0));
    assert_eq!(result.reason(), "out of stock");
    assert_eq!(
        result.title.as_deref(),
        Some("OPPO Find X9 (256 GB Storage, 12 GB RAM)")
    );
}

#[test]
fn in_stock_and_serviceable_is_available() {
    let result = parse_page_json(&support::fixture("in_stock.json"), &product()).unwrap();
    assert!(result.available);
    assert!(result.in_stock);
    assert!(result.deliverable);
    assert_eq!(result.price_text(), "₹79,999");
    assert_eq!(result.reason(), "available");
}

#[test]
fn in_stock_but_not_serviceable_is_not_available() {
    let mut payload = support::fixture("in_stock.json");
    payload["RESPONSE"]["pageData"]["pageContext"]["fdpEventTracking"]["events"]["psi"]["pls"]
        ["isServiceable"] = json!(false);
    payload["RESPONSE"]["pageData"]["pageContext"]["fdpEventTracking"]["events"]["psi"]["pls"]
        ["serviceable"] = json!(false);
    let result = parse_page_json(&payload, &product()).unwrap();
    assert!(result.in_stock);
    assert!(!result.available);
    assert_eq!(result.reason(), "not deliverable to this pincode");
}

#[test]
fn no_pincode_is_an_error() {
    let mut payload = support::fixture("out_of_stock.json");
    payload["RESPONSE"]["pageData"]["pageContext"]["fdpEventTracking"]["events"]["psi"]["pls"]
        ["unserviceabilityReason"] = json!("NO_PINCODE");
    let error = parse_page_json(&payload, &product()).unwrap_err();
    assert!(matches!(error, FlipkartError::PincodeNotApplied));
}

#[test]
fn missing_psi_is_an_error() {
    let payload = json!({"RESPONSE": {"pageData": {}}});
    let error = parse_page_json(&payload, &product()).unwrap_err();
    assert!(matches!(error, FlipkartError::MissingPayload));
}

#[test]
fn coupon_amount_is_parsed_from_offer() {
    let result = parse_page_json(&support::fixture("coupon_offer.json"), &product()).unwrap();
    assert_eq!(result.coupon, Some(10000.0));
    assert!(result.available);
    assert!(result.qualifies());
}

#[test]
fn accessory_coupon_is_not_mistaken_for_a_phone_coupon() {
    let mut payload = support::fixture("coupon_offer.json");
    payload["RESPONSE"]["slots"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    assert_eq!(best_coupon(&payload), None);
    assert_eq!(parse_page_json(&payload, &product()).unwrap().coupon, None);
}

#[test]
fn x9s_requires_a_10000_coupon() {
    let mut payload = support::fixture("coupon_offer.json");
    payload["RESPONSE"]["pageData"]["pageContext"]["fdpEventTracking"]["events"]["psi"]["pls"]
        ["isServiceable"] = json!(true);
    let result = parse_page_json(&payload, &x9s()).unwrap();
    assert!(result.qualifies());
    assert_eq!(result.reason(), "available");
}

#[test]
fn x9s_without_coupon_does_not_qualify() {
    let mut payload = support::fixture("coupon_offer.json");
    payload["RESPONSE"]["slots"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    payload["RESPONSE"]["pageData"]["pageContext"]["fdpEventTracking"]["events"]["psi"]["pls"]
        ["isServiceable"] = json!(true);
    let result = parse_page_json(&payload, &x9s()).unwrap();
    assert!(result.available);
    assert_eq!(result.coupon, None);
    assert!(!result.qualifies());
    assert_eq!(result.reason(), "no ₹10,000 coupon");
}

#[test]
fn x9s_with_smaller_coupon_does_not_qualify() {
    let mut payload = support::fixture("coupon_offer.json");
    let offer = &mut payload["RESPONSE"]["slots"][0]["widget"]["data"]["offers"][0]["value"]
        ["offerSummariesRC"][0]["value"];
    offer["discountedPriceText"] = json!("₹9,000 off");
    offer["offerSubTitleRC"]["value"]["contentList"][0]["contentValue"] =
        json!("Save ₹9,000 extra on 1 item(s)");
    let result = parse_page_json(&payload, &x9s()).unwrap();
    assert_eq!(result.coupon, Some(9000.0));
    assert!(!result.qualifies());
    assert_eq!(result.reason(), "no ₹10,000 coupon");
}

#[test]
fn percentage_coupon_is_ignored() {
    let mut payload = support::fixture("coupon_offer.json");
    let offer = &mut payload["RESPONSE"]["slots"][0]["widget"]["data"]["offers"][0]["value"]
        ["offerSummariesRC"][0]["value"];
    offer["discountedPriceText"] = json!("10% off");
    offer["offerSubTitleRC"]["value"]["contentList"][0]["contentValue"] =
        json!("10% off on 1 item(s)");
    assert_eq!(best_coupon(&payload), None);
}

#[test]
fn x9_listing_ignores_coupon_gate() {
    let result = parse_page_json(&support::fixture("coupon_offer.json"), &product()).unwrap();
    assert_eq!(result.product.min_coupon, None);
    assert_eq!(result.qualifies(), result.available);
}
