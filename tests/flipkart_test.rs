mod support;

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use fkwatch::flipkart::{
    FlipkartClient, FlipkartError, HttpRequest, HttpResponse, Transport, TransportError,
    LOCATION_UPDATE_URL, PAGE_FETCH_URL,
};
use fkwatch::models::Product;

#[derive(Default)]
struct Recorder {
    calls: Vec<HttpRequest>,
    resets: usize,
}

struct FakeTransport {
    responses: VecDeque<HttpResponse>,
    recorder: Arc<Mutex<Recorder>>,
}

type SharedRecorder = Arc<Mutex<Recorder>>;
type SharedSleeps = Arc<Mutex<Vec<f64>>>;

impl Transport for FakeTransport {
    fn execute(&mut self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        self.recorder.lock().unwrap().calls.push(request.clone());
        let response = self
            .responses
            .pop_front()
            .expect("unexpected extra request");
        Ok(response)
    }

    fn reset(&mut self) {
        self.recorder.lock().unwrap().resets += 1;
    }
}

fn product() -> Product {
    Product::new(
        "Space Black",
        "/oppo-find-x9-space-black-256-gb/p/itma96fe4cab1395",
        "MOBHGZZ8GH2Z85MY",
    )
}

fn response(payload: Value) -> HttpResponse {
    HttpResponse {
        status: 200,
        body: payload.to_string(),
    }
}

fn empty_ok() -> HttpResponse {
    HttpResponse {
        status: 200,
        body: "{}".to_string(),
    }
}

fn status(code: u16) -> HttpResponse {
    HttpResponse {
        status: code,
        body: String::new(),
    }
}

fn client(
    responses: Vec<HttpResponse>,
    retries: usize,
    backoff: f64,
) -> (FlipkartClient, SharedRecorder, SharedSleeps) {
    let recorder = Arc::new(Mutex::new(Recorder::default()));
    let transport = Box::new(FakeTransport {
        responses: responses.into(),
        recorder: Arc::clone(&recorder),
    });
    let sleeps = Arc::new(Mutex::new(Vec::new()));
    let sleeper = Arc::clone(&sleeps);
    let client = FlipkartClient::with_transport(
        "412101",
        retries,
        backoff,
        transport,
        Box::new(move |seconds| sleeper.lock().unwrap().push(seconds)),
    )
    .unwrap();
    (client, recorder, sleeps)
}

fn header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

fn no_pincode_payload() -> Value {
    let mut payload = support::fixture("out_of_stock.json");
    payload["RESPONSE"]["pageData"]["pageContext"]["fdpEventTracking"]["events"]["psi"]["pls"]
        ["unserviceabilityReason"] = json!("NO_PINCODE");
    payload
}

#[test]
fn happy_path_binds_pincode_and_returns_availability() {
    let (mut client, recorder, _) = client(
        vec![
            empty_ok(),
            empty_ok(),
            response(support::fixture("in_stock.json")),
        ],
        3,
        0.0,
    );
    let result = client.check(&product());

    assert!(result.available);
    assert_eq!(result.price, Some(79999.0));

    let recorder = recorder.lock().unwrap();
    let calls = &recorder.calls;
    assert_eq!(
        calls.iter().map(|call| call.method).collect::<Vec<_>>(),
        ["POST", "GET", "POST"]
    );
    assert_eq!(calls[0].url, LOCATION_UPDATE_URL);
    assert_eq!(calls[1].url, product().url());
    assert_eq!(calls[2].url, PAGE_FETCH_URL);

    let location_body = calls[0].body.as_ref().unwrap();
    assert_eq!(location_body["addressInfo"]["pincode"], json!("412101"));
    assert!(header(&calls[0], "x-user-agent")
        .unwrap()
        .ends_with("FKUA/msite/0.0.4/msite/Mobile"));

    let fetch_body = calls[2].body.as_ref().unwrap();
    assert_eq!(
        fetch_body["pageUri"],
        json!(format!("{}?pid={}", product().path, product().pid))
    );
    assert_eq!(fetch_body["locationContext"]["pincode"], json!(412101));
}

#[test]
fn retries_transient_status() {
    let (mut client, _, sleeps) = client(
        vec![
            empty_ok(),
            empty_ok(),
            status(406),
            response(support::fixture("in_stock.json")),
        ],
        2,
        0.5,
    );
    let result = client.check(&product());
    assert!(result.ok() && result.available);
    assert_eq!(*sleeps.lock().unwrap(), vec![0.5]);
}

#[test]
fn restarts_session_and_retries_when_pincode_not_applied() {
    let (mut client, recorder, _) = client(
        vec![
            empty_ok(),
            empty_ok(),
            response(no_pincode_payload()),
            empty_ok(),
            empty_ok(),
            response(support::fixture("in_stock.json")),
        ],
        3,
        0.0,
    );
    let result = client.check(&product());
    assert!(result.available);

    let recorder = recorder.lock().unwrap();
    assert_eq!(
        recorder
            .calls
            .iter()
            .filter(|call| call.method == "GET")
            .count(),
        2
    );
    assert_eq!(
        recorder
            .calls
            .iter()
            .filter(|call| call.url == LOCATION_UPDATE_URL)
            .count(),
        2
    );
    assert_eq!(recorder.resets, 1);
}

#[test]
fn missing_payload_restarts_session_and_retries() {
    let (mut client, _, _) = client(
        vec![
            empty_ok(),
            empty_ok(),
            response(json!({"RESPONSE": {"pageData": {}}})),
            empty_ok(),
            empty_ok(),
            response(support::fixture("in_stock.json")),
        ],
        3,
        0.0,
    );
    let result = client.check(&product());
    assert!(result.ok() && result.available);
}

#[test]
fn pincode_still_not_applied_returns_error() {
    let (mut client, recorder, _) = client(
        vec![
            empty_ok(),
            empty_ok(),
            response(no_pincode_payload()),
            empty_ok(),
            empty_ok(),
            response(no_pincode_payload()),
        ],
        3,
        0.0,
    );
    let result = client.check(&product());
    assert!(!result.available);
    let error = result.error.expect("expected an error");
    assert!(error.contains("NO_PINCODE"));
    assert_eq!(recorder.lock().unwrap().resets, 1);
}

#[test]
fn location_update_with_wrong_pincode_is_an_error() {
    let (mut client, _, _) = client(
        vec![response(json!({"RESPONSE": {"currentPincode": "400001"}}))],
        3,
        0.0,
    );
    let result = client.check(&product());
    assert!(!result.available);
    let error = result.error.expect("expected an error");
    assert!(error.contains("400001"));
}

#[test]
fn prime_failure_returns_error_not_false_negative() {
    let (mut client, _, _) = client(vec![status(500)], 1, 0.0);
    let result = client.check(&product());
    assert!(result.error.is_some());
    assert!(!result.ok());
}

#[test]
fn invalid_json_is_an_error() {
    let (mut client, _, _) = client(
        vec![
            empty_ok(),
            empty_ok(),
            HttpResponse {
                status: 200,
                body: "not json".to_string(),
            },
        ],
        3,
        0.0,
    );
    let result = client.check(&product());
    let error = result.error.expect("expected an error");
    assert!(error.contains("non-JSON"));
}

#[test]
fn one_session_handles_multiple_products() {
    let second = Product::new(
        "Velvet Red",
        "/oppo-find-x9-velvet-red-256-gb/p/itma96fe4cab1395",
        "MOBHG8Y56JNQGGTX",
    );
    let (mut client, recorder, _) = client(
        vec![
            empty_ok(),
            empty_ok(),
            response(support::fixture("out_of_stock.json")),
            response(support::fixture("in_stock.json")),
        ],
        3,
        0.0,
    );
    let results = client.check_all(&[product(), second]);
    assert_eq!(results.len(), 2);
    assert!(!results[0].available);
    assert!(results[1].available);
    let recorder = recorder.lock().unwrap();
    assert_eq!(
        recorder
            .calls
            .iter()
            .filter(|call| call.method == "GET")
            .count(),
        1
    );
}

#[test]
fn rejects_bad_pincode() {
    let result = FlipkartClient::new("12345", Duration::from_secs(1), 3, 0.0);
    assert!(matches!(result, Err(FlipkartError::InvalidPincode(_))));
}
