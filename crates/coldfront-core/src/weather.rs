// SPDX-License-Identifier: GPL-3.0-only

/// Short condition label and an icon id shared with the web app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WeatherInfo {
    pub label: &'static str,
    pub icon: &'static str,
}

#[must_use]
pub fn describe(code: i32) -> WeatherInfo {
    match code {
        0 => WeatherInfo {
            label: "Clear",
            icon: "clear",
        },
        1 => WeatherInfo {
            label: "Mainly clear",
            icon: "mainly-clear",
        },
        2 => WeatherInfo {
            label: "Partly cloudy",
            icon: "partly-cloudy",
        },
        3 => WeatherInfo {
            label: "Overcast",
            icon: "overcast",
        },
        45 | 48 => WeatherInfo {
            label: "Fog",
            icon: "fog",
        },
        51 | 53 | 55 => WeatherInfo {
            label: "Drizzle",
            icon: "drizzle",
        },
        56 | 57 => WeatherInfo {
            label: "Freezing drizzle",
            icon: "freezing-drizzle",
        },
        61 | 63 | 65 => WeatherInfo {
            label: "Rain",
            icon: "rain",
        },
        66 | 67 => WeatherInfo {
            label: "Freezing rain",
            icon: "freezing-rain",
        },
        71 | 73 | 75 | 77 => WeatherInfo {
            label: "Snow",
            icon: "snow",
        },
        80..=82 => WeatherInfo {
            label: "Rain showers",
            icon: "rain-showers",
        },
        85 | 86 => WeatherInfo {
            label: "Snow showers",
            icon: "snow-showers",
        },
        95 | 96 | 99 => WeatherInfo {
            label: "Thunderstorm",
            icon: "thunder",
        },
        _ => WeatherInfo {
            label: "Unknown",
            icon: "unknown",
        },
    }
}

/// Freedesktop icon name for a Coldfront icon id. COSMIC falls back if the
/// name is missing from the icon theme; the text label still shows.
#[must_use]
pub fn symbolic_icon(icon: &str) -> &'static str {
    match icon {
        "clear" | "mainly-clear" => "weather-clear-symbolic",
        "partly-cloudy" => "weather-few-clouds-symbolic",
        "overcast" => "weather-overcast-symbolic",
        "fog" => "weather-fog-symbolic",
        "snow" | "snow-showers" => "weather-snow-symbolic",
        "thunder" => "weather-storm-symbolic",
        _ => "weather-showers-symbolic",
    }
}
