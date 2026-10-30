use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::ser::SerializeStruct;
use serde::{Deserialize, Serialize, Serializer};

use crate::models::RunReport;

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct StateEntry {
    #[serde(default)]
    pub available: bool,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub error: Option<String>,
}

impl Serialize for StateEntry {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Python wrote the state with sort_keys=True, so emit the fields in
        // alphabetical order to keep existing state files byte-compatible.
        let mut state = serializer.serialize_struct("StateEntry", 3)?;
        state.serialize_field("available", &self.available)?;
        state.serialize_field("error", &self.error)?;
        state.serialize_field("price", &self.price)?;
        state.end()
    }
}

pub type State = BTreeMap<String, StateEntry>;

#[derive(Debug, thiserror::Error)]
pub enum StateError {
    #[error("could not write state: {0}")]
    Io(#[from] std::io::Error),
    #[error("could not serialize state: {0}")]
    Json(#[from] serde_json::Error),
}

pub fn snapshot(report: &RunReport) -> State {
    report
        .results
        .iter()
        .map(|result| {
            (
                result.product.pid.clone(),
                StateEntry {
                    available: result.available,
                    price: result.price,
                    error: result.error.clone(),
                },
            )
        })
        .collect()
}

pub fn load_state(path: &Path) -> State {
    match fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
        Err(_) => State::new(),
    }
}

pub fn save_state(path: &Path, report: &RunReport) -> Result<(), StateError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let text = serde_json::to_string_pretty(&snapshot(report))?;
    fs::write(path, text)?;
    Ok(())
}

pub fn is_changed(previous: &State, report: &RunReport) -> bool {
    if previous.is_empty() {
        return true;
    }
    let current = snapshot(report);
    for (pid, state) in &current {
        let Some(old) = previous.get(pid) else {
            return true;
        };
        if state.error.is_some() {
            continue;
        }
        if old.error.is_some() {
            // Recovered from an error: the real status is news again.
            return true;
        }
        if state.available != old.available {
            return true;
        }
        if state.price != old.price {
            return true;
        }
    }
    false
}
