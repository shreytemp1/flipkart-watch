use chrono::FixedOffset;
use reqwest::blocking::Client;
use serde_json::{json, Value};

use crate::models::{CheckResult, RunReport};

pub const IST_OFFSET_SECONDS: i32 = 5 * 3600 + 30 * 60;
pub const SAFE_CHUNK: usize = 3800;

#[derive(Debug, thiserror::Error)]
pub enum NotificationError {
    #[error("could not build Telegram client: {0}")]
    Client(String),
    #[error("telegram request failed: {0}")]
    Request(String),
    #[error("Telegram rejected the message: HTTP {status} {body}")]
    Rejected { status: u16, body: String },
}

pub fn char_len(text: &str) -> usize {
    text.chars().count()
}

fn escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn escape_quote(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#x27;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn link(result: &CheckResult) -> String {
    let url = escape_quote(&result.product.url());
    format!("<a href=\"{url}\">Buy now on Flipkart</a>")
}

pub fn render_message(report: &RunReport, device: &str) -> String {
    let offset = FixedOffset::east_opt(IST_OFFSET_SECONDS).expect("valid IST offset");
    let stamp = report
        .checked_at
        .with_timezone(&offset)
        .format("%Y-%m-%d %H:%M IST")
        .to_string();
    let head = format!(
        "📍 {}\nPincode <b>{}</b>",
        escape(&report.address),
        escape(&report.pincode)
    );

    let mut lines: Vec<String>;
    if report.any_available() {
        lines = vec![
            format!(
                "✅ <b>{}</b> — AVAILABLE at {}",
                escape(device),
                escape(&report.pincode)
            ),
            head,
            String::new(),
        ];
        for result in report.available() {
            let mut line = format!(
                "• <b>{}</b> — <b>{}</b>",
                escape(&result.product.name),
                result.price_text()
            );
            if result.product.min_coupon.is_some() {
                if let Some(coupon) = result.coupon {
                    line.push_str(&format!(
                        " — coupon <b>₹{} off</b>",
                        crate::models::format_amount(coupon)
                    ));
                }
            }
            lines.push(format!("{line}\n   {}", link(result)));
        }
        let others: Vec<&CheckResult> = report
            .results
            .iter()
            .filter(|result| result.ok() && !result.qualifies())
            .collect();
        if !others.is_empty() {
            lines.push(String::new());
            lines.push("<i>Not available:</i>".to_string());
            for result in others {
                lines.push(format!(
                    "• {} — {} — {}",
                    escape(&result.product.name),
                    escape(&result.reason()),
                    result.price_text()
                ));
            }
        }
    } else if !report.results.iter().any(CheckResult::ok) {
        lines = vec![
            format!(
                "⚠️ <b>{}</b> — COULD NOT CHECK at {}",
                escape(device),
                escape(&report.pincode)
            ),
            head,
            String::new(),
        ];
    } else {
        lines = vec![
            format!(
                "❌ <b>{}</b> — NOT AVAILABLE at {}",
                escape(device),
                escape(&report.pincode)
            ),
            head,
            String::new(),
        ];
        for result in &report.results {
            if result.ok() {
                lines.push(format!(
                    "• {} — {} — {}",
                    escape(&result.product.name),
                    escape(&result.reason()),
                    result.price_text()
                ));
            }
        }
    }

    if !report.errors().is_empty() {
        lines.push(String::new());
        lines.push("⚠️ <i>Could not check:</i>".to_string());
        for result in report.errors() {
            lines.push(format!(
                "• {} — {}",
                escape(&result.product.name),
                escape(result.error.as_deref().unwrap_or("unknown error"))
            ));
        }
    }

    lines.push(String::new());
    lines.push(format!("🕒 {stamp}"));
    lines.join("\n")
}

pub fn chunk_message(text: &str, limit: usize) -> Vec<String> {
    if char_len(text) <= limit {
        return vec![text.to_string()];
    }
    let mut chunks = Vec::new();
    let mut current: Vec<&str> = Vec::new();
    let mut size = 0usize;
    for line in text.split('\n') {
        let addition = char_len(line) + 1;
        if !current.is_empty() && size + addition > limit {
            chunks.push(current.join("\n"));
            current.clear();
            size = 0;
        }
        current.push(line);
        size += addition;
    }
    if !current.is_empty() {
        chunks.push(current.join("\n"));
    }
    chunks
}

pub fn telegram_payload(chat_id: &str, text: &str) -> Value {
    json!({
        "chat_id": chat_id,
        "text": text,
        "parse_mode": "HTML",
        "disable_web_page_preview": true,
    })
}

pub struct TelegramNotifier {
    token: String,
    chat_id: String,
    api_base: String,
    client: Client,
}

impl TelegramNotifier {
    pub fn new(
        token: impl Into<String>,
        chat_id: impl Into<String>,
        api_base: impl Into<String>,
    ) -> Result<Self, NotificationError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|error| NotificationError::Client(error.to_string()))?;
        Ok(Self {
            token: token.into(),
            chat_id: chat_id.into(),
            api_base: api_base.into().trim_end_matches('/').to_string(),
            client,
        })
    }

    pub fn send(&self, text: &str) -> Result<usize, NotificationError> {
        let mut sent = 0;
        for chunk in chunk_message(text, SAFE_CHUNK) {
            let url = format!("{}/bot{}/sendMessage", self.api_base, self.token);
            let response = self
                .client
                .post(&url)
                .json(&telegram_payload(&self.chat_id, &chunk))
                .send()
                .map_err(|error| NotificationError::Request(error.to_string()))?;
            let status = response.status().as_u16();
            let body = response
                .text()
                .map_err(|error| NotificationError::Request(error.to_string()))?;
            if status != 200 {
                return Err(NotificationError::Rejected {
                    status,
                    body: body.chars().take(200).collect(),
                });
            }
            let ok = serde_json::from_str::<Value>(&body)
                .ok()
                .and_then(|value| value.get("ok").and_then(Value::as_bool))
                .unwrap_or(false);
            if !ok {
                return Err(NotificationError::Rejected {
                    status,
                    body: body.chars().take(200).collect(),
                });
            }
            sent += 1;
        }
        Ok(sent)
    }
}
