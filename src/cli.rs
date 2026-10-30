use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{SecondsFormat, Utc};
use clap::Parser;
use log::{error, info, warn, LevelFilter};
use serde::Serialize;

use crate::config::{Config, NotifyMode};
use crate::flipkart::{FlipkartClient, FlipkartError};
use crate::models::RunReport;
use crate::notify::{char_len, render_message, TelegramNotifier};
use crate::state::{is_changed, load_state, save_state, State};

#[derive(Debug, Parser)]
#[command(
    name = "fkwatch",
    about = "Check Flipkart Find X9 listings for stock and pincode delivery, then notify Telegram."
)]
struct Args {
    /// Path to config.yaml.
    #[arg(long, default_value = "config.yaml")]
    config: PathBuf,
    /// Print the message instead of sending it.
    #[arg(long)]
    dry_run: bool,
    /// Write a JSON report here.
    #[arg(long, default_value = "result.json")]
    json_out: PathBuf,
    /// Do not write the JSON report.
    #[arg(long)]
    no_json: bool,
    /// State file for notify.mode=changes.
    #[arg(long, default_value = ".state/fkwatch.json")]
    state: PathBuf,
    /// Debug logging.
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Serialize)]
struct JsonProduct<'a> {
    name: &'a str,
    pid: &'a str,
    url: String,
    min_coupon: Option<f64>,
}

#[derive(Serialize)]
struct JsonResult<'a> {
    product: JsonProduct<'a>,
    available: bool,
    in_stock: bool,
    deliverable: bool,
    price: Option<f64>,
    mrp: Option<f64>,
    title: Option<&'a str>,
    unserviceability_reason: Option<&'a str>,
    coupon: Option<f64>,
    error: Option<&'a str>,
    qualifies: bool,
}

#[derive(Serialize)]
struct JsonReport<'a> {
    pincode: &'a str,
    address: &'a str,
    checked_at: String,
    any_available: bool,
    results: Vec<JsonResult<'a>>,
}

pub fn build_report(config: &Config) -> Result<RunReport, FlipkartError> {
    let mut client = FlipkartClient::new(
        &config.pincode,
        Duration::from_secs_f64(config.http.timeout_seconds),
        config.http.retries,
        config.http.backoff_seconds,
    )?;
    let results = client.check_all(&config.products);
    Ok(RunReport {
        pincode: config.pincode.clone(),
        address: config.address.clone(),
        results,
        checked_at: Utc::now(),
    })
}

pub fn report_to_json(report: &RunReport) -> serde_json::Value {
    let results = report
        .results
        .iter()
        .map(|result| JsonResult {
            product: JsonProduct {
                name: &result.product.name,
                pid: &result.product.pid,
                url: result.product.url(),
                min_coupon: result.product.min_coupon,
            },
            available: result.available,
            in_stock: result.in_stock,
            deliverable: result.deliverable,
            price: result.price,
            mrp: result.mrp,
            title: result.title.as_deref(),
            unserviceability_reason: result.unserviceability_reason.as_deref(),
            coupon: result.coupon,
            error: result.error.as_deref(),
            qualifies: result.qualifies(),
        })
        .collect();
    let output = JsonReport {
        pincode: &report.pincode,
        address: &report.address,
        checked_at: report
            .checked_at
            .to_rfc3339_opts(SecondsFormat::Micros, false),
        any_available: report.any_available(),
        results,
    };
    serde_json::to_value(output).expect("report is serializable")
}

pub fn should_notify(mode: NotifyMode, report: &RunReport, previous: &State) -> bool {
    match mode {
        NotifyMode::Always => true,
        NotifyMode::Changes => is_changed(previous, report),
        NotifyMode::Available => report.any_available(),
    }
}

fn init_logging(verbose: bool) {
    env_logger::Builder::new()
        .filter_level(if verbose {
            LevelFilter::Debug
        } else {
            LevelFilter::Info
        })
        .format(|buffer, record| {
            writeln!(
                buffer,
                "{} {} {}: {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.target(),
                record.args()
            )
        })
        .init();
}

fn write_json(path: &Path, report: &RunReport) -> Result<(), String> {
    let text =
        serde_json::to_string_pretty(&report_to_json(report)).map_err(|error| error.to_string())?;
    fs::write(path, text).map_err(|error| error.to_string())
}

fn py_bool(value: bool) -> &'static str {
    if value {
        "True"
    } else {
        "False"
    }
}

pub fn run() -> i32 {
    let args = Args::parse();
    init_logging(args.verbose);

    let config = match Config::load(&args.config) {
        Ok(config) => config,
        Err(error) => {
            error!("configuration error: {error}");
            return 2;
        }
    };

    let report = match build_report(&config) {
        Ok(report) => report,
        Err(error) => {
            error!("could not complete any check: {error}");
            return 2;
        }
    };

    if !args.no_json {
        if let Err(error) = write_json(&args.json_out, &report) {
            error!("could not write {}: {error}", args.json_out.display());
            return 2;
        }
    }

    let message = render_message(&report, &config.device);
    let previous = load_state(&args.state);
    let should_send = should_notify(config.notify.mode, &report, &previous);
    if !should_send {
        match config.notify.mode {
            NotifyMode::Changes => info!("no availability/price change; skipping notification"),
            NotifyMode::Available => info!("no qualifying listing; skipping notification"),
            NotifyMode::Always => {}
        }
    }

    if args.dry_run {
        println!("{message}\n");
        println!(
            "[dry-run] {} {} chars; mode={}; available={}; errors={}",
            if should_send {
                "would send"
            } else {
                "would NOT send"
            },
            char_len(&message),
            config.notify.mode.as_str(),
            py_bool(report.any_available()),
            report.errors().len()
        );
    } else if should_send {
        let token = env::var("TELEGRAM_BOT_TOKEN")
            .unwrap_or_default()
            .trim()
            .to_string();
        let chat_id = env::var("TELEGRAM_CHAT_ID")
            .unwrap_or_default()
            .trim()
            .to_string();
        if token.is_empty() || chat_id.is_empty() {
            error!("TELEGRAM_BOT_TOKEN and TELEGRAM_CHAT_ID must be set");
            return 3;
        }
        let sent = TelegramNotifier::new(&token, &chat_id, &config.telegram.api_base)
            .and_then(|notifier| notifier.send(&message));
        match sent {
            Ok(count) => info!("sent {count} Telegram message(s)"),
            Err(error) => {
                error!("telegram delivery failed: {error}");
                return 3;
            }
        }
    }

    if !args.no_json && !args.dry_run {
        if let Err(error) = save_state(&args.state, &report) {
            error!("{error}");
            return 2;
        }
    }

    let errors = report.errors().len();
    if errors == report.results.len() {
        error!("every listing failed to check");
        return 2;
    }
    if errors > 0 {
        warn!(
            "{} of {} listings could not be checked",
            errors,
            report.results.len()
        );
        return 1;
    }
    0
}
