use std::fmt;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_yaml::Value;

use crate::models::Product;

#[derive(Debug)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyMode {
    Always,
    Changes,
    Available,
}

impl NotifyMode {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "always" => Some(Self::Always),
            "changes" => Some(Self::Changes),
            "available" => Some(Self::Available),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Always => "always",
            Self::Changes => "changes",
            Self::Available => "available",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NotifyConfig {
    pub mode: NotifyMode,
}

impl Default for NotifyConfig {
    fn default() -> Self {
        Self {
            mode: NotifyMode::Always,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HttpConfig {
    pub timeout_seconds: f64,
    pub retries: usize,
    pub backoff_seconds: f64,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            timeout_seconds: 30.0,
            retries: 3,
            backoff_seconds: 2.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramConfig {
    pub api_base: String,
}

impl Default for TelegramConfig {
    fn default() -> Self {
        Self {
            api_base: "https://api.telegram.org".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub pincode: String,
    pub address: String,
    pub device: String,
    pub products: Vec<Product>,
    pub notify: NotifyConfig,
    pub http: HttpConfig,
    pub telegram: TelegramConfig,
}

#[derive(Debug, Deserialize, Default)]
struct RawConfig {
    #[serde(default)]
    pincode: Option<Value>,
    #[serde(default)]
    address: Option<RawAddress>,
    #[serde(default)]
    products: Vec<RawProduct>,
    #[serde(default)]
    device: Option<Value>,
    #[serde(default)]
    notify: Option<RawNotify>,
    #[serde(default)]
    http: Option<RawHttp>,
    #[serde(default)]
    telegram: Option<RawTelegram>,
}

#[derive(Debug, Deserialize, Default)]
struct RawAddress {
    #[serde(default)]
    line1: Option<Value>,
    #[serde(default)]
    city: Option<Value>,
    #[serde(default)]
    state: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
struct RawProduct {
    #[serde(default)]
    name: Option<Value>,
    #[serde(default)]
    path: Option<Value>,
    #[serde(default)]
    pid: Option<Value>,
    #[serde(default)]
    min_coupon: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
struct RawNotify {
    #[serde(default)]
    mode: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
struct RawHttp {
    #[serde(default)]
    timeout_seconds: Option<Value>,
    #[serde(default)]
    retries: Option<Value>,
    #[serde(default)]
    backoff_seconds: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
struct RawTelegram {
    #[serde(default)]
    api_base: Option<Value>,
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim()
            .to_string(),
    }
}

fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64(),
        Value::String(text) => text.trim().parse::<f64>().ok(),
        _ => None,
    }
}

fn value_with_default(value: &Option<Value>, default: &str) -> String {
    match value {
        Some(value) => value_to_string(value),
        None => default.to_string(),
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = fs::read_to_string(path)
            .map_err(|error| ConfigError(format!("{}: {error}", path.display())))?;
        let raw: RawConfig = serde_yaml::from_str(&text)
            .map_err(|error| ConfigError(format!("{}: {error}", path.display())))?;

        let pincode = raw
            .pincode
            .as_ref()
            .map(value_to_string)
            .unwrap_or_default()
            .trim()
            .to_string();
        if !(pincode.len() == 6 && pincode.chars().all(|ch| ch.is_ascii_digit())) {
            return Err(ConfigError(format!(
                "{}: pincode must be a 6-digit string, got '{pincode}'",
                path.display()
            )));
        }

        let address = raw
            .address
            .as_ref()
            .map(|address| {
                [&address.line1, &address.city, &address.state]
                    .into_iter()
                    .filter_map(|value| value.as_ref().map(value_to_string))
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();

        if raw.products.is_empty() {
            return Err(ConfigError(format!(
                "{}: at least one product is required",
                path.display()
            )));
        }

        let mut products = Vec::with_capacity(raw.products.len());
        for (index, item) in raw.products.iter().enumerate() {
            let path_value = item
                .path
                .as_ref()
                .map(value_to_string)
                .unwrap_or_default()
                .trim()
                .to_string();
            let pid = item
                .pid
                .as_ref()
                .map(value_to_string)
                .unwrap_or_default()
                .trim()
                .to_string();
            if path_value.is_empty() || pid.is_empty() {
                return Err(ConfigError(format!(
                    "{}: products[{index}] needs both path and pid",
                    path.display()
                )));
            }

            let min_coupon = match &item.min_coupon {
                None | Some(Value::Null) => None,
                Some(value) => {
                    let parsed = value_as_f64(value).ok_or_else(|| {
                        ConfigError(format!(
                            "{}: products[{index}].min_coupon must be a number",
                            path.display()
                        ))
                    })?;
                    if parsed <= 0.0 {
                        return Err(ConfigError(format!(
                            "{}: products[{index}].min_coupon must be greater than zero",
                            path.display()
                        )));
                    }
                    Some(parsed)
                }
            };

            products.push(Product {
                name: value_with_default(&item.name, &format!("listing {}", index + 1))
                    .trim()
                    .to_string(),
                path: if path_value.starts_with('/') {
                    path_value
                } else {
                    format!("/{path_value}")
                },
                pid,
                min_coupon,
            });
        }

        let mode_raw = raw
            .notify
            .as_ref()
            .and_then(|notify| notify.mode.as_ref())
            .map(value_to_string)
            .unwrap_or_else(|| "always".to_string());
        let mode = NotifyMode::parse(&mode_raw).ok_or_else(|| {
            ConfigError(format!(
                "{}: notify.mode must be 'always', 'changes' or 'available', got '{}'",
                path.display(),
                mode_raw.trim().to_ascii_lowercase()
            ))
        })?;

        let http_defaults = HttpConfig::default();
        let http = raw.http.as_ref();
        let timeout_seconds = http
            .and_then(|http| http.timeout_seconds.as_ref())
            .and_then(value_as_f64)
            .unwrap_or(http_defaults.timeout_seconds);
        let retries = http
            .and_then(|http| http.retries.as_ref())
            .and_then(value_as_f64)
            .map(|value| value.max(1.0) as usize)
            .unwrap_or(http_defaults.retries);
        let backoff_seconds = http
            .and_then(|http| http.backoff_seconds.as_ref())
            .and_then(value_as_f64)
            .unwrap_or(http_defaults.backoff_seconds);

        let api_base = raw
            .telegram
            .as_ref()
            .and_then(|telegram| telegram.api_base.as_ref())
            .map(value_to_string)
            .unwrap_or_else(|| "https://api.telegram.org".to_string())
            .trim_end_matches('/')
            .to_string();

        Ok(Self {
            pincode,
            address,
            device: value_with_default(&raw.device, "OPPO Find X9 12+256")
                .trim()
                .to_string(),
            products,
            notify: NotifyConfig { mode },
            http: HttpConfig {
                timeout_seconds,
                retries,
                backoff_seconds,
            },
            telegram: TelegramConfig { api_base },
        })
    }
}
