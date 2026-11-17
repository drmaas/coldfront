// SPDX-License-Identifier: GPL-3.0-only

use crate::error::Error;
use jiff::civil::{Date, Weekday};
use serde_json::Value;

/// Open-Meteo forecast normalized to Celsius, km/h, and WMO weather codes.
#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    pub latitude: f64,
    pub longitude: f64,
    pub timezone: String,
    pub current: Current,
    pub hourly: Vec<Hour>,
    pub daily: Vec<Day>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Current {
    pub time: String,
    pub temperature_c: f64,
    pub apparent_c: f64,
    pub humidity: i32,
    pub wind_kmh: f64,
    pub weather_code: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hour {
    pub time: String,
    pub temperature_c: f64,
    pub weather_code: i32,
    pub precip_probability: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Day {
    pub date: String,
    pub weather_code: i32,
    pub temp_max_c: f64,
    pub temp_min_c: f64,
    pub precip_probability: i32,
}

/// Parse an Open-Meteo forecast response that uses Celsius and km/h.
pub fn parse_open_meteo(bytes: &[u8]) -> Result<Forecast, Error> {
    let value: Value =
        serde_json::from_slice(bytes).map_err(|err| Error::Parse(err.to_string()))?;
    let current = value
        .get("current")
        .ok_or_else(|| Error::Parse("missing current".into()))?;
    let hourly = value
        .get("hourly")
        .ok_or_else(|| Error::Parse("missing hourly".into()))?;
    let daily = value
        .get("daily")
        .ok_or_else(|| Error::Parse("missing daily".into()))?;

    let hour_times = string_array(hourly, "time")?;
    let hour_temps = number_array(hourly, "temperature_2m")?;
    let hour_codes = number_array(hourly, "weather_code")?;
    let hour_pops = number_array(hourly, "precipitation_probability")?;
    if hour_times.len() != hour_temps.len()
        || hour_times.len() != hour_codes.len()
        || hour_times.len() != hour_pops.len()
    {
        return Err(Error::Parse("hourly length mismatch".into()));
    }

    let day_times = string_array(daily, "time")?;
    let day_codes = number_array(daily, "weather_code")?;
    let day_max = number_array(daily, "temperature_2m_max")?;
    let day_min = number_array(daily, "temperature_2m_min")?;
    let day_pops = number_array(daily, "precipitation_probability_max")?;
    if day_times.len() != day_codes.len()
        || day_times.len() != day_max.len()
        || day_times.len() != day_min.len()
        || day_times.len() != day_pops.len()
    {
        return Err(Error::Parse("daily length mismatch".into()));
    }

    let hourly_rows = hour_times
        .into_iter()
        .zip(hour_temps)
        .zip(hour_codes)
        .zip(hour_pops)
        .map(|(((time, temperature_c), weather_code), precip)| Hour {
            time,
            temperature_c,
            weather_code: round_code(weather_code),
            precip_probability: round_code(precip),
        })
        .collect();
    let daily_rows = day_times
        .into_iter()
        .zip(day_codes)
        .zip(day_max)
        .zip(day_min)
        .zip(day_pops)
        .map(
            |((((date, weather_code), temp_max_c), temp_min_c), precip)| Day {
                date,
                weather_code: round_code(weather_code),
                temp_max_c,
                temp_min_c,
                precip_probability: round_code(precip),
            },
        )
        .collect();

    Ok(Forecast {
        latitude: number(&value, "latitude")?,
        longitude: number(&value, "longitude")?,
        timezone: string_field(&value, "timezone")?,
        current: Current {
            time: string_field(current, "time")?,
            temperature_c: number(current, "temperature_2m")?,
            apparent_c: number(current, "apparent_temperature")?,
            humidity: round_code(number(current, "relative_humidity_2m")?),
            wind_kmh: number(current, "wind_speed_10m")?,
            weather_code: round_code(number(current, "weather_code")?),
        },
        hourly: hourly_rows,
        daily: daily_rows,
    })
}

/// Hours from the current local hour forward.
#[must_use]
pub fn next_hours(forecast: &Forecast, count: usize) -> Vec<&Hour> {
    let start = forecast
        .hourly
        .iter()
        .rposition(|hour| hour.time.as_str() <= forecast.current.time.as_str())
        .unwrap_or(0);
    forecast.hourly.iter().skip(start).take(count).collect()
}

/// `4:45 PM` from an Open-Meteo local timestamp `2026-11-17T16:45`.
#[must_use]
pub fn format_as_of(time: &str) -> String {
    let Some(hour) = clock_hour(time) else {
        return time.to_string();
    };
    let minute = time.get(14..16).unwrap_or("00");
    let suffix = if hour >= 12 { "PM" } else { "AM" };
    let hour12 = match hour % 12 {
        0 => 12,
        value => value,
    };
    format!("{hour12}:{minute} {suffix}")
}

/// `Now`, `5p`, or `12a`.
#[must_use]
pub fn hour_label(time: &str, is_now: bool) -> String {
    if is_now {
        return "Now".to_string();
    }
    let Some(hour) = clock_hour(time) else {
        return time.to_string();
    };
    match hour {
        0 => "12a".to_string(),
        12 => "12p".to_string(),
        1..=11 => format!("{hour}a"),
        _ => format!("{}p", hour.saturating_sub(12)),
    }
}

/// `Today` or a short weekday. `today` is `YYYY-MM-DD`.
#[must_use]
pub fn day_label(date: &str, today: &str) -> String {
    if date == today {
        return "Today".to_string();
    }
    let Ok(parsed) = date.parse::<Date>() else {
        return date.to_string();
    };
    match parsed.weekday() {
        Weekday::Sunday => "Sun",
        Weekday::Monday => "Mon",
        Weekday::Tuesday => "Tue",
        Weekday::Wednesday => "Wed",
        Weekday::Thursday => "Thu",
        Weekday::Friday => "Fri",
        Weekday::Saturday => "Sat",
    }
    .to_string()
}

fn clock_hour(time: &str) -> Option<u32> {
    time.get(11..13)?.parse().ok()
}

fn round_code(value: f64) -> i32 {
    crate::units::round_half_away_from_zero(value)
}

fn string_field(value: &Value, key: &str) -> Result<String, Error> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| Error::Parse(format!("missing {key}")))
}

fn number(value: &Value, key: &str) -> Result<f64, Error> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .ok_or_else(|| Error::Parse(format!("missing {key}")))
}

fn string_array(value: &Value, key: &str) -> Result<Vec<String>, Error> {
    let array = value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Parse(format!("missing {key}")))?;
    array
        .iter()
        .map(|item| {
            item.as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| Error::Parse(format!("bad {key}")))
        })
        .collect()
}

fn number_array(value: &Value, key: &str) -> Result<Vec<f64>, Error> {
    let array = value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Parse(format!("missing {key}")))?;
    array
        .iter()
        .map(|item| {
            if item.is_null() {
                return Ok(0.0);
            }
            item.as_f64()
                .ok_or_else(|| Error::Parse(format!("bad {key}")))
        })
        .collect()
}
