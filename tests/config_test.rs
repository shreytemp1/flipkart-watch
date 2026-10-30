use std::collections::BTreeSet;
use std::path::PathBuf;

use fkwatch::config::{Config, NotifyMode};
use tempfile::tempdir;

fn repo_config() -> Config {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("config.yaml");
    Config::load(&path).expect("repository config should load")
}

#[test]
fn repository_config_is_valid() {
    let config = repo_config();
    assert_eq!(config.pincode, "412101");
    assert!(config.address.contains("Pimpri-Chinchwad"));
    assert_eq!(config.products.len(), 4);
    let pids: BTreeSet<&str> = config
        .products
        .iter()
        .map(|product| product.pid.as_str())
        .collect();
    assert_eq!(
        pids,
        BTreeSet::from([
            "MOBHGZZ8GH2Z85MY",
            "MOBHGZZ8SAMGGJHR",
            "MOBHKZVXT93DXHMP",
            "MOBHG8Y56JNQGGTX",
        ])
    );
    assert_eq!(config.notify.mode, NotifyMode::Available);
    assert_eq!(config.device, "OPPO Find X9 12+256");
    assert!(config
        .products
        .iter()
        .all(|product| product.min_coupon.is_none()));
}

#[test]
fn product_url_is_built() {
    let config = repo_config();
    assert_eq!(
        config.products[0].url(),
        "https://www.flipkart.com/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395?pid=MOBHGZZ8GH2Z85MY"
    );
}

#[test]
fn rejects_short_pincode() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("c.yaml");
    std::fs::write(
        &path,
        "pincode: '41210'\nproducts: [{path: /x, pid: ABC}]\n",
    )
    .unwrap();
    assert!(Config::load(&path).is_err());
}

#[test]
fn rejects_empty_products() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("c.yaml");
    std::fs::write(&path, "pincode: '412101'\nproducts: []\n").unwrap();
    assert!(Config::load(&path).is_err());
}

#[test]
fn rejects_unknown_notify_mode() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("c.yaml");
    std::fs::write(
        &path,
        "pincode: '412101'\nnotify: {mode: sometimes}\nproducts: [{path: /x, pid: ABC}]\n",
    )
    .unwrap();
    let error = Config::load(&path).unwrap_err().to_string();
    assert!(error.contains("notify.mode"));
}

#[test]
fn parses_min_coupon() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("c.yaml");
    std::fs::write(
        &path,
        "pincode: '412101'\nproducts:\n  - {path: /x, pid: ABC, min_coupon: 10000}\n  - {path: /y, pid: DEF}\n",
    )
    .unwrap();
    let config = Config::load(&path).unwrap();
    assert_eq!(config.products[0].min_coupon, Some(10000.0));
    assert_eq!(config.products[1].min_coupon, None);
}

#[test]
fn rejects_invalid_min_coupon() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("c.yaml");
    std::fs::write(
        &path,
        "pincode: '412101'\nproducts: [{path: /x, pid: ABC, min_coupon: lots}]\n",
    )
    .unwrap();
    assert!(Config::load(&path).is_err());

    std::fs::write(
        &path,
        "pincode: '412101'\nproducts: [{path: /x, pid: ABC, min_coupon: 0}]\n",
    )
    .unwrap();
    assert!(Config::load(&path).is_err());
}
