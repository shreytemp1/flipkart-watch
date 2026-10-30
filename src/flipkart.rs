use std::sync::OnceLock;
use std::time::Duration;

use log::{info, warn};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, ACCEPT_LANGUAGE, USER_AGENT as UA_HEADER};
use reqwest::Method;
use serde_json::{json, Map, Value};

use crate::models::{CheckResult, Product};

pub const BASE: &str = "https://www.flipkart.com";
pub const USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
pub const X_USER_AGENT: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 FKUA/msite/0.0.4/msite/Mobile";
pub const PAGE_FETCH_URL: &str = "https://www.flipkart.com/api/4/page/fetch?cacheFirst=false";
pub const LOCATION_UPDATE_URL: &str = "https://www.flipkart.com/api/4/location/update";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    LocationUpdate,
    ProductPage,
    PageFetch,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HttpRequest {
    pub kind: RequestKind,
    pub method: &'static str,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct TransportError(pub String);

pub trait Transport {
    fn execute(&mut self, request: &HttpRequest) -> Result<HttpResponse, TransportError>;
    fn reset(&mut self);
}

#[derive(Debug, thiserror::Error)]
pub enum FlipkartError {
    #[error("invalid pincode: {0}")]
    InvalidPincode(String),
    #[error("request failed after {attempts} attempts: {last}")]
    RequestFailed { attempts: usize, last: String },
    #[error("HTTP {status} from {url}")]
    HttpStatus { status: u16, url: String },
    #[error("location update failed: HTTP {0}")]
    LocationUpdate(u16),
    #[error("location update bound pincode {actual:?}, expected {expected:?}")]
    PincodeMismatch { actual: String, expected: String },
    #[error("Flipkart answered with NO_PINCODE (location binding failed)")]
    PincodeNotApplied,
    #[error("response did not contain the expected 'psi.pls' payload")]
    MissingPayload,
    #[error("non-JSON response (HTTP {0})")]
    NonJson(u16),
    #[error("transport: {0}")]
    Transport(String),
}

struct ReqwestTransport {
    timeout: Duration,
    client: Client,
}

impl ReqwestTransport {
    fn new(timeout: Duration) -> Result<Self, FlipkartError> {
        Ok(Self {
            timeout,
            client: Self::build(timeout)?,
        })
    }

    fn build(timeout: Duration) -> Result<Client, FlipkartError> {
        let mut headers = HeaderMap::new();
        headers.insert(UA_HEADER, HeaderValue::from_static(USER_AGENT));
        headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("en-IN,en;q=0.9"));
        headers.insert(ACCEPT, HeaderValue::from_static("*/*"));

        Client::builder()
            .timeout(timeout)
            .cookie_store(true)
            .default_headers(headers)
            .build()
            .map_err(|error| FlipkartError::Transport(error.to_string()))
    }
}

impl Transport for ReqwestTransport {
    fn execute(&mut self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        let method = Method::from_bytes(request.method.as_bytes())
            .map_err(|error| TransportError(error.to_string()))?;
        let mut builder = self.client.request(method, &request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name, value);
        }
        if let Some(body) = &request.body {
            builder = builder.json(body);
        }
        let response = builder
            .send()
            .map_err(|error| TransportError(error.to_string()))?;
        let status = response.status().as_u16();
        let body = response
            .text()
            .map_err(|error| TransportError(error.to_string()))?;
        Ok(HttpResponse { status, body })
    }

    fn reset(&mut self) {
        if let Ok(client) = Self::build(self.timeout) {
            self.client = client;
        }
    }
}

pub struct FlipkartClient {
    pincode: String,
    retries: usize,
    backoff: f64,
    transport: Box<dyn Transport>,
    sleeper: Box<dyn Fn(f64)>,
    primed: bool,
}

impl FlipkartClient {
    pub fn new(
        pincode: &str,
        timeout: Duration,
        retries: usize,
        backoff: f64,
    ) -> Result<Self, FlipkartError> {
        let transport = Box::new(ReqwestTransport::new(timeout)?);
        Self::with_transport(
            pincode,
            retries,
            backoff,
            transport,
            Box::new(|seconds| std::thread::sleep(Duration::from_secs_f64(seconds))),
        )
    }

    pub fn with_transport(
        pincode: impl Into<String>,
        retries: usize,
        backoff: f64,
        transport: Box<dyn Transport>,
        sleeper: Box<dyn Fn(f64)>,
    ) -> Result<Self, FlipkartError> {
        let pincode = pincode.into();
        if !(pincode.len() == 6 && pincode.chars().all(|ch| ch.is_ascii_digit())) {
            return Err(FlipkartError::InvalidPincode(pincode));
        }
        Ok(Self {
            pincode,
            retries: retries.max(1),
            backoff,
            transport,
            sleeper,
            primed: false,
        })
    }

    fn request(&mut self, request: HttpRequest) -> Result<HttpResponse, FlipkartError> {
        let mut last = String::new();
        for attempt in 1..=self.retries {
            let outcome = self.transport.execute(&request);
            match outcome {
                Ok(response) => {
                    if matches!(response.status, 406 | 429 | 500 | 502 | 503 | 504) {
                        last = format!("HTTP {} from {}", response.status, request.url);
                    } else {
                        return Ok(response);
                    }
                }
                Err(error) => last = error.to_string(),
            }
            if attempt == self.retries {
                break;
            }
            warn!("attempt {}/{} failed: {}", attempt, self.retries, last);
            (self.sleeper)(self.backoff * attempt as f64);
        }
        Err(FlipkartError::RequestFailed {
            attempts: self.retries,
            last,
        })
    }

    fn reset_session(&mut self) {
        self.transport.reset();
        self.primed = false;
    }

    /// Bind the pincode *before* loading the product page. With the reverse
    /// order Flipkart answered NO_PINCODE for roughly half of the sessions in
    /// testing; bind-then-load was reliable (16/16).
    fn prime(&mut self, product: &Product) -> Result<(), FlipkartError> {
        if self.primed {
            return Ok(());
        }
        self.bind_pincode()?;
        self.request(HttpRequest {
            kind: RequestKind::ProductPage,
            method: "GET",
            url: product.url(),
            headers: Vec::new(),
            body: None,
        })?;
        self.primed = true;
        Ok(())
    }

    fn bind_pincode(&mut self) -> Result<(), FlipkartError> {
        let body = json!({
            "geoLocation": {"latitude": "18.6742528", "longitude": "73.7301331"},
            "addressInfo": {
                "addressLine1": "Vikas Nagar",
                "city": "Pimpri-Chinchwad",
                "state": "Maharashtra",
                "pincode": self.pincode,
            },
            "redirectionUrl": "",
            "marketplace": "FLIPKART",
        });
        let response = self.request(HttpRequest {
            kind: RequestKind::LocationUpdate,
            method: "POST",
            url: LOCATION_UPDATE_URL.to_string(),
            headers: vec![
                ("x-user-agent".to_string(), X_USER_AGENT.to_string()),
                ("referer".to_string(), format!("{BASE}/")),
            ],
            body: Some(body),
        })?;
        if response.status >= 400 {
            return Err(FlipkartError::LocationUpdate(response.status));
        }
        if let Ok(payload) = serde_json::from_str::<Value>(&response.body) {
            if let Some(current) = payload
                .get("RESPONSE")
                .and_then(|response| response.get("currentPincode"))
            {
                if !current.is_null() {
                    let actual = match current {
                        Value::String(text) => text.clone(),
                        other => other.to_string(),
                    };
                    if actual != self.pincode {
                        return Err(FlipkartError::PincodeMismatch {
                            actual,
                            expected: self.pincode.clone(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    fn fetch(&mut self, product: &Product) -> Result<Value, FlipkartError> {
        let pincode = self
            .pincode
            .parse::<i64>()
            .map_err(|_| FlipkartError::InvalidPincode(self.pincode.clone()))?;
        let body = json!({
            "pageUri": format!("{}?pid={}", product.path, product.pid),
            "pageContext": {"trackingContext": {}, "networkSpeed": 10000},
            "locationContext": {"pincode": pincode, "changed": false},
        });
        let response = self.request(HttpRequest {
            kind: RequestKind::PageFetch,
            method: "POST",
            url: PAGE_FETCH_URL.to_string(),
            headers: vec![
                ("x-user-agent".to_string(), X_USER_AGENT.to_string()),
                ("referer".to_string(), format!("{BASE}/")),
                ("origin".to_string(), BASE.to_string()),
            ],
            body: Some(body),
        })?;
        serde_json::from_str::<Value>(&response.body)
            .map_err(|_| FlipkartError::NonJson(response.status))
    }

    pub fn check(&mut self, product: &Product) -> CheckResult {
        if let Err(error) = self.prime(product) {
            return CheckResult::with_error(product.clone(), error.to_string());
        }

        let mut last_error: Option<FlipkartError> = None;
        for attempt in 0..2 {
            let outcome = self
                .fetch(product)
                .and_then(|payload| parse_page_json(&payload, product));
            match outcome {
                Ok(result) => return result,
                Err(error) => {
                    let retryable = matches!(
                        &error,
                        FlipkartError::PincodeNotApplied | FlipkartError::MissingPayload
                    );
                    if !retryable {
                        return CheckResult::with_error(product.clone(), error.to_string());
                    }
                    if attempt == 0 {
                        info!(
                            "{} for {}, restarting session and retrying once",
                            error, product.pid
                        );
                    }
                    last_error = Some(error);
                    if attempt == 1 {
                        break;
                    }
                    self.reset_session();
                    if let Err(prime_error) = self.prime(product) {
                        return CheckResult::with_error(product.clone(), prime_error.to_string());
                    }
                }
            }
        }
        let message = last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "unknown error".to_string());
        CheckResult::with_error(product.clone(), message)
    }

    pub fn check_all(&mut self, products: &[Product]) -> Vec<CheckResult> {
        let mut results = Vec::with_capacity(products.len());
        for product in products {
            info!("checking {} ({})", product.name, product.pid);
            results.push(self.check(product));
        }
        results
    }
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().is_some_and(|number| number != 0.0),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

fn base_payload(payload: &Value) -> &Value {
    match payload.get("RESPONSE") {
        Some(response) if truthy(response) => response,
        _ => payload,
    }
}

fn psi(payload: &Value) -> Result<&Map<String, Value>, FlipkartError> {
    let value = base_payload(payload)
        .get("pageData")
        .and_then(|page| page.get("pageContext"))
        .and_then(|context| context.get("fdpEventTracking"))
        .and_then(|tracking| tracking.get("events"))
        .and_then(|events| events.get("psi"));
    match value {
        Some(Value::Object(map)) if map.contains_key("pls") => Ok(map),
        _ => Err(FlipkartError::MissingPayload),
    }
}

fn title(payload: &Value) -> Option<String> {
    let seo_title = base_payload(payload)
        .get("pageData")?
        .get("seoData")?
        .get("seo")?
        .get("title")?
        .as_str()?;
    if seo_title.trim().is_empty() {
        return None;
    }
    let short = seo_title
        .split(" Online at Best Price")
        .next()
        .unwrap_or(seo_title)
        .trim();
    Some(short.to_string())
}

static COUPON_AMOUNT_RE: OnceLock<Regex> = OnceLock::new();

fn coupon_regex() -> &'static Regex {
    COUPON_AMOUNT_RE.get_or_init(|| Regex::new(r"₹\s*([\d,]+)").expect("valid coupon regex"))
}

/// Rupee-denominated texts belonging to one coupon offer.
fn coupon_texts(offer: &Map<String, Value>) -> Vec<&str> {
    let mut texts = Vec::new();
    if let Some(text) = offer.get("discountedPriceText").and_then(Value::as_str) {
        texts.push(text);
    }
    if let Some(value) = offer
        .get("offerSubTitleRC")
        .and_then(|subtitle| subtitle.get("value"))
    {
        if let Some(items) = value.get("contentList").and_then(Value::as_array) {
            for item in items {
                if let Some(text) = item.get("contentValue").and_then(Value::as_str) {
                    texts.push(text);
                }
            }
        }
    }
    texts
}

/// Largest flat rupee coupon on the page (percentage coupons are ignored).
///
/// Flipkart exposes user-selectable coupons as offers tagged with
/// `offerType == "USER_COUPON_SELECTION"` (or titled `Coupons`), for
/// example `discountedPriceText` "₹9000 off" with the subtitle
/// "Save ₹9,000 extra on 1 item(s)". Accessory coupons such as
/// "Unlock Oppo coupon • ₹1499 off" live elsewhere and are deliberately
/// not treated as phone coupons.
pub fn best_coupon(payload: &Value) -> Option<f64> {
    fn walk(node: &Value, regex: &Regex, best: &mut Option<f64>) {
        match node {
            Value::Object(map) => {
                let is_coupon = map.get("offerType").and_then(Value::as_str)
                    == Some("USER_COUPON_SELECTION")
                    || map.get("offerTitle").and_then(Value::as_str) == Some("Coupons");
                if is_coupon {
                    for text in coupon_texts(map) {
                        if text.contains('%') {
                            continue;
                        }
                        for captures in regex.captures_iter(text) {
                            let raw = captures[1].replace(',', "");
                            if let Ok(amount) = raw.parse::<f64>() {
                                if best.is_none_or(|current| amount > current) {
                                    *best = Some(amount);
                                }
                            }
                        }
                    }
                }
                for value in map.values() {
                    walk(value, regex, best);
                }
            }
            Value::Array(items) => {
                for item in items {
                    walk(item, regex, best);
                }
            }
            _ => {}
        }
    }

    let mut best = None;
    walk(payload, coupon_regex(), &mut best);
    best
}

pub fn parse_page_json(payload: &Value, product: &Product) -> Result<CheckResult, FlipkartError> {
    let psi = psi(payload)?;
    let pls = psi
        .get("pls")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let ppd = psi
        .get("ppd")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let reason = pls
        .get("unserviceabilityReason")
        .and_then(|value| match value {
            Value::Null => None,
            Value::String(text) => Some(text.clone()),
            other => Some(other.to_string()),
        });
    if reason.as_deref() == Some("NO_PINCODE") {
        return Err(FlipkartError::PincodeNotApplied);
    }

    let in_stock = pls.get("availabilityStatus").and_then(Value::as_str) == Some("IN_STOCK")
        || pls.get("isAvailable").and_then(Value::as_bool) == Some(true);
    let deliverable = pls.get("isServiceable").and_then(Value::as_bool) == Some(true);
    let price = ppd.get("finalPrice").and_then(Value::as_f64);
    let mrp = ppd.get("mrp").and_then(Value::as_f64);

    Ok(CheckResult {
        product: product.clone(),
        available: in_stock && deliverable,
        in_stock,
        deliverable,
        price,
        mrp,
        title: title(payload),
        unserviceability_reason: reason,
        coupon: best_coupon(payload),
        error: None,
    })
}

/// Convenience for tests and offline parsing.
pub fn check_from_payload(payload: &Value, product: &Product) -> CheckResult {
    match parse_page_json(payload, product) {
        Ok(result) => result,
        Err(error) => CheckResult::with_error(product.clone(), error.to_string()),
    }
}
