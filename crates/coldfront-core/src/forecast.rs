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
    /// Open-Meteo day/night flag for the place timezone (`1` day, `0` night).
    pub is_day: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hour {
    pub time: String,
    pub temperature_c: f64,
    pub weather_code: i32,
    pub precip_probability: Option<i32>,
    pub is_day: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Day {
    pub date: String,
    pub weather_code: i32,
    pub temp_max_c: f64,
    pub temp_min_c: f64,
    pub precip_probability: i32,
    pub precip_sum_mm: Option<f64>,
    pub wind_max_kmh: Option<f64>,
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub humidity_mean: Option<i32>,
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
    let hour_pops = optional_precip_array(hourly, "precipitation_probability", hour_times.len())?;
    let hour_is_day = number_array(hourly, "is_day")?;
    if hour_times.len() != hour_temps.len()
        || hour_times.len() != hour_codes.len()
        || hour_times.len() != hour_pops.len()
        || hour_times.len() != hour_is_day.len()
    {
        return Err(Error::Parse("hourly length mismatch".into()));
    }

    let day_times = string_array(daily, "time")?;
    let day_codes = number_array(daily, "weather_code")?;
    let day_max = number_array(daily, "temperature_2m_max")?;
    let day_min = number_array(daily, "temperature_2m_min")?;
    let day_pops = number_array(daily, "precipitation_probability_max")?;
    let day_sums = optional_number_array(daily, "precipitation_sum", day_times.len());
    let day_winds = optional_number_array(daily, "wind_speed_10m_max", day_times.len());
    let day_sunrise = optional_string_array(daily, "sunrise", day_times.len());
    let day_sunset = optional_string_array(daily, "sunset", day_times.len());
    let day_humidity = optional_number_array(daily, "relative_humidity_2m_mean", day_times.len());
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
        .zip(hour_is_day)
        .map(
            |((((time, temperature_c), weather_code), precip_probability), is_day)| Hour {
                time,
                temperature_c,
                weather_code: round_code(weather_code),
                precip_probability,
                is_day: is_day != 0.0,
            },
        )
        .collect();
    let daily_rows = day_times
        .into_iter()
        .enumerate()
        .map(|(index, date)| {
            let weather_code = round_code(day_codes[index]);
            Day {
                date,
                weather_code,
                temp_max_c: day_max[index],
                temp_min_c: day_min[index],
                precip_probability: round_code(day_pops[index]),
                precip_sum_mm: day_sums[index],
                wind_max_kmh: day_winds[index],
                sunrise: day_sunrise[index].clone(),
                sunset: day_sunset[index].clone(),
                humidity_mean: day_humidity[index].map(round_code),
            }
        })
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
            is_day: number(current, "is_day")? != 0.0,
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
    format_clock_time(time).unwrap_or_else(|| time.to_string())
}

/// `7:24 AM` from a local timestamp, or `None` when the string is not a clock time.
#[must_use]
pub fn format_clock_time(time: &str) -> Option<String> {
    let hour = clock_hour(time)?;
    let minute = time.get(14..16).unwrap_or("00");
    let suffix = if hour >= 12 { "PM" } else { "AM" };
    let hour12 = match hour % 12 {
        0 => 12,
        value => value,
    };
    Some(format!("{hour12}:{minute} {suffix}"))
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

/// Daily precipitation total for display.
#[must_use]
pub fn format_precip_amount_mm(mm: f64, fahrenheit: bool) -> String {
    if fahrenheit {
        let inches = crate::units::round_half_away_from_zero(mm / 25.4 * 100.0) as f64 / 100.0;
        format!("{inches:.2} in")
    } else {
        let rounded = crate::units::round_half_away_from_zero(mm * 10.0) as f64 / 10.0;
        format!("{rounded:.1} mm")
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

fn optional_precip_array(value: &Value, key: &str, len: usize) -> Result<Vec<Option<i32>>, Error> {
    let Some(array) = value.get(key).and_then(Value::as_array) else {
        return Ok(vec![None; len]);
    };
    if array.len() != len {
        return Err(Error::Parse(format!("bad {key}")));
    }
    array
        .iter()
        .map(|item| {
            if item.is_null() {
                return Ok(None);
            }
            let value = item
                .as_f64()
                .ok_or_else(|| Error::Parse(format!("bad {key}")))?;
            Ok(Some(round_code(value)))
        })
        .collect()
}

fn optional_number_array(value: &Value, key: &str, len: usize) -> Vec<Option<f64>> {
    let Some(array) = value.get(key).and_then(Value::as_array) else {
        return vec![None; len];
    };
    if array.len() != len {
        return vec![None; len];
    }
    array
        .iter()
        .map(|item| {
            if item.is_null() {
                None
            } else {
                item.as_f64().filter(|value| value.is_finite())
            }
        })
        .collect()
}

fn optional_string_array(value: &Value, key: &str, len: usize) -> Vec<Option<String>> {
    let Some(array) = value.get(key).and_then(Value::as_array) else {
        return vec![None; len];
    };
    if array.len() != len {
        return vec![None; len];
    }
    array
        .iter()
        .map(|item| {
            item.as_str()
                .filter(|text| !text.is_empty())
                .map(ToOwned::to_owned)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hourly_precipitation_probability_handles_null_entries() {
        let body = br#"{
            "latitude": 0,
            "longitude": 0,
            "timezone": "UTC",
            "current": {
                "time": "2026-11-17T12:00",
                "temperature_2m": 10,
                "apparent_temperature": 10,
                "relative_humidity_2m": 50,
                "wind_speed_10m": 5,
                "weather_code": 1,
                "is_day": 1
            },
            "hourly": {
                "time": ["2026-11-17T12:00", "2026-11-17T13:00"],
                "temperature_2m": [10, 11],
                "weather_code": [1, 1],
                "precipitation_probability": [25, null],
                "is_day": [1, 1]
            },
            "daily": {
                "time": ["2026-11-17"],
                "weather_code": [1],
                "temperature_2m_max": [12],
                "temperature_2m_min": [5],
                "precipitation_probability_max": [10]
            }
        }"#;
        let forecast = parse_open_meteo(body).expect("forecast");
        assert_eq!(forecast.hourly[0].precip_probability, Some(25));
        assert_eq!(forecast.hourly[1].precip_probability, None);
    }

    #[test]
    fn daily_optional_fields_are_absent_when_missing() {
        let body = br#"{
            "latitude": 0,
            "longitude": 0,
            "timezone": "UTC",
            "current": {
                "time": "2026-11-17T12:00",
                "temperature_2m": 10,
                "apparent_temperature": 10,
                "relative_humidity_2m": 50,
                "wind_speed_10m": 5,
                "weather_code": 1,
                "is_day": 1
            },
            "hourly": {
                "time": ["2026-11-17T12:00"],
                "temperature_2m": [10],
                "weather_code": [1],
                "precipitation_probability": [0],
                "is_day": [1]
            },
            "daily": {
                "time": ["2026-11-17", "2026-11-18"],
                "weather_code": [1, 2],
                "temperature_2m_max": [12, 11],
                "temperature_2m_min": [5, 4],
                "precipitation_probability_max": [10, 20],
                "sunrise": ["2026-11-17T07:12", null],
                "wind_speed_10m_max": [18.2, null]
            }
        }"#;
        let forecast = parse_open_meteo(body).expect("forecast");
        let first = &forecast.daily[0];
        assert_eq!(first.sunrise.as_deref(), Some("2026-11-17T07:12"));
        assert_eq!(first.wind_max_kmh, Some(18.2));
        assert_eq!(first.precip_sum_mm, None);
        assert_eq!(forecast.daily[1].sunrise, None);
        assert_eq!(forecast.daily[1].wind_max_kmh, None);
        assert_eq!(first.humidity_mean, None);
        assert_eq!(
            format_clock_time(first.sunrise.as_deref().unwrap()),
            Some("7:12 AM".into())
        );
    }

    #[test]
    fn daily_mean_humidity_rounds_and_missing_stays_none() {
        let body = br#"{
            "latitude": 0,
            "longitude": 0,
            "timezone": "UTC",
            "current": {
                "time": "2026-11-17T12:00",
                "temperature_2m": 10,
                "apparent_temperature": 10,
                "relative_humidity_2m": 50,
                "wind_speed_10m": 5,
                "weather_code": 1,
                "is_day": 1
            },
            "hourly": {
                "time": ["2026-11-17T12:00"],
                "temperature_2m": [10],
                "weather_code": [1],
                "precipitation_probability": [0],
                "is_day": [1]
            },
            "daily": {
                "time": ["2026-11-17", "2026-11-18"],
                "weather_code": [1, 2],
                "temperature_2m_max": [12, 11],
                "temperature_2m_min": [5, 4],
                "precipitation_probability_max": [10, 20],
                "relative_humidity_2m_mean": [58.4, null]
            }
        }"#;
        let forecast = parse_open_meteo(body).expect("forecast");
        assert_eq!(forecast.daily[0].humidity_mean, Some(58));
        assert_eq!(forecast.daily[1].humidity_mean, None);
    }
}
