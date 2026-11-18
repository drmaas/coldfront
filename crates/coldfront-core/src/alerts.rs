// SPDX-License-Identifier: GPL-3.0-only

use crate::error::Error;
use serde_json::Value;

/// One active National Weather Service alert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub id: String,
    pub event: String,
    pub headline: String,
    pub severity: String,
    pub urgency: String,
    pub description: String,
    pub instruction: String,
    pub onset: String,
    pub ends: String,
    pub sender_name: String,
}

/// Parse an `api.weather.gov` alert FeatureCollection.
pub fn parse_alerts(bytes: &[u8]) -> Result<Vec<Alert>, Error> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|err| Error::Parse(err.to_string()))?;
    let features = value
        .get("features")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Parse("missing features".into()))?;
    let mut alerts = Vec::new();
    for feature in features {
        let Some(properties) = feature.get("properties") else {
            continue;
        };
        let event = text(properties, "event");
        if event.is_empty() {
            continue;
        }
        alerts.push(Alert {
            id: feature
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            event,
            headline: text(properties, "headline"),
            severity: text(properties, "severity"),
            urgency: text(properties, "urgency"),
            description: text(properties, "description"),
            instruction: text(properties, "instruction"),
            onset: text(properties, "onset"),
            ends: text(properties, "ends"),
            sender_name: text(properties, "senderName"),
        });
    }
    Ok(alerts)
}

fn text(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}
