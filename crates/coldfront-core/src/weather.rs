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

/// Day or night display icon id from a `describe` icon and Open-Meteo `is_day`.
///
/// Clear / mainly clear / partly cloudy swap to moon variants at night. Other
/// conditions keep the day icon.
#[must_use]
pub fn display_icon(icon: &str, is_day: bool) -> &'static str {
    if !is_day {
        match icon {
            "clear" => return "clear-night",
            "mainly-clear" => return "mainly-clear-night",
            "partly-cloudy" => return "partly-cloudy-night",
            _ => {}
        }
    }
    match icon {
        "clear" => "clear",
        "mainly-clear" => "mainly-clear",
        "partly-cloudy" => "partly-cloudy",
        "overcast" => "overcast",
        "fog" => "fog",
        "drizzle" => "drizzle",
        "freezing-drizzle" => "freezing-drizzle",
        "rain" => "rain",
        "freezing-rain" => "freezing-rain",
        "snow" => "snow",
        "rain-showers" => "rain-showers",
        "snow-showers" => "snow-showers",
        "thunder" => "thunder",
        "clear-night" => "clear-night",
        "mainly-clear-night" => "mainly-clear-night",
        "partly-cloudy-night" => "partly-cloudy-night",
        _ => "unknown",
    }
}

/// Freedesktop icon name for a Coldfront icon id. COSMIC falls back if the
/// name is missing from the icon theme; the text label still shows.
#[must_use]
pub fn symbolic_icon(icon: &str) -> &'static str {
    match icon {
        "clear" | "mainly-clear" => "weather-clear-symbolic",
        "clear-night" | "mainly-clear-night" => "weather-clear-night-symbolic",
        "partly-cloudy" => "weather-few-clouds-symbolic",
        "partly-cloudy-night" => "weather-few-clouds-night-symbolic",
        "overcast" => "weather-overcast-symbolic",
        "fog" => "weather-fog-symbolic",
        "drizzle" | "rain" | "rain-showers" | "freezing-drizzle" | "freezing-rain" => {
            "weather-showers-symbolic"
        }
        "snow" | "snow-showers" => "weather-snow-symbolic",
        "thunder" => "weather-storm-symbolic",
        _ => "weather-showers-symbolic",
    }
}

#[cfg(test)]
mod tests {
    use super::symbolic_icon;

    #[test]
    fn symbolic_icon_maps_precip_types() {
        for icon in ["drizzle", "rain", "rain-showers", "freezing-drizzle", "freezing-rain"] {
            assert_eq!(symbolic_icon(icon), "weather-showers-symbolic");
        }
        for icon in ["snow", "snow-showers"] {
            assert_eq!(symbolic_icon(icon), "weather-snow-symbolic");
        }
    }
}
