from pathlib import Path

import pytest

from fkwatch.config import Config, ConfigError

ROOT = Path(__file__).parent.parent


def test_repository_config_is_valid():
    config = Config.load(ROOT / "config.yaml")
    assert config.pincode == "412101"
    assert "Pimpri-Chinchwad" in config.address
    assert len(config.products) == 4
    assert {p.pid for p in config.products} == {
        "MOBHGZZ8GH2Z85MY",
        "MOBHGZZ8SAMGGJHR",
        "MOBHKZVXT93DXHMP",
        "MOBHG8Y56JNQGGTX",
    }
    assert config.notify.mode == "available"


def test_product_url_is_built():
    config = Config.load(ROOT / "config.yaml")
    assert config.products[0].url == (
        "https://www.flipkart.com/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395?pid=MOBHGZZ8GH2Z85MY"
    )


def test_rejects_short_pincode(tmp_path):
    path = tmp_path / "c.yaml"
    path.write_text("pincode: '41210'\nproducts: [{path: /x, pid: ABC}]\n")
    with pytest.raises(ConfigError):
        Config.load(path)


def test_rejects_empty_products(tmp_path):
    path = tmp_path / "c.yaml"
    path.write_text("pincode: '412101'\nproducts: []\n")
    with pytest.raises(ConfigError):
        Config.load(path)


def test_rejects_unknown_notify_mode(tmp_path):
    path = tmp_path / "c.yaml"
    path.write_text("pincode: '412101'\nnotify: {mode: sometimes}\nproducts: [{path: /x, pid: ABC}]\n")
    with pytest.raises(ConfigError):
        Config.load(path)
