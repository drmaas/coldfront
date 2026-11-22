// SPDX-License-Identifier: GPL-3.0-only

//! Both clients run these fixtures. The numbers live in `fixtures/contract.json`.

use std::fs;
use std::path::PathBuf;

use coldfront_core::{
    OrthoFit, RadarLookback, RadarSpeed, RouteTarget, applet_radar_frames_at,
    applet_radar_frames_for, applet_radar_interval_ms, band_height, chart_points, day_label,
    describe,
    display_icon, display_temperature, display_wind_mph, fit_ortho_half_extents, format_as_of,
    format_clock_time, hour_label, iem_tile_url, is_us_zip, is_zip_lookup, lat_lon_to_tile,
    location_path, next_hours, parse_alerts, parse_geocode_search, parse_open_meteo,
    parse_route_path, parse_zippopotam, place_from_geocode_hit, range_placement,
    round_half_away_from_zero, tile_window, web_radar_frames_at, web_radar_frames_for,
    web_radar_interval_ms,
};
use serde_json::Value;

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures")
        .join(name);
    fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn contract() -> Value {
    serde_json::from_slice(&fixture("contract.json")).expect("contract json")
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
}

#[test]
fn geocode_and_location_rules_match_the_contract() {
    let contract = contract();
    let geocode = &contract["geocode"];
    assert!(!is_zip_lookup("Eureka, CA"));
    assert!(is_zip_lookup(geocode["eureka_ca_zip"].as_str().unwrap()));
    let hits = parse_geocode_search(&fixture("geocode-eureka.json")).unwrap();
    let place = place_from_geocode_hit(&hits[0]);
    assert_eq!(place.zip, geocode["eureka_ca_zip"].as_str().unwrap());
    assert_eq!(place.city, geocode["eureka_ca_city"].as_str().unwrap());
    assert_eq!(
        place.state_abbreviation,
        geocode["eureka_ca_state"].as_str().unwrap()
    );
    close(
        place.latitude,
        geocode["eureka_ca_latitude"].as_f64().unwrap(),
    );
    close(
        place.longitude,
        geocode["eureka_ca_longitude"].as_f64().unwrap(),
    );
    assert_eq!(location_path(&place), format!("/{}", place.zip));
    assert_eq!(parse_route_path("/95501"), RouteTarget::Zip("95501".into()));
}

#[test]
fn zip_rules_match_the_contract() {
    let zips = &contract()["zips"];
    for value in zips["valid"].as_array().unwrap() {
        assert!(is_us_zip(value.as_str().unwrap()), "{value}");
    }
    for value in zips["invalid"].as_array().unwrap() {
        assert!(!is_us_zip(value.as_str().unwrap()), "{value}");
    }
}

#[test]
fn place_forecast_and_labels_match_the_contract() {
    let contract = contract();
    let place = parse_zippopotam(&fixture("zippopotam-55115.json")).unwrap();
    let expected_place = &contract["place"];
    assert_eq!(place.zip, expected_place["zip"].as_str().unwrap());
    assert_eq!(place.city, expected_place["city"].as_str().unwrap());
    assert_eq!(place.state, expected_place["state"].as_str().unwrap());
    assert_eq!(
        place.state_abbreviation,
        expected_place["state_abbreviation"].as_str().unwrap()
    );
    close(place.latitude, expected_place["latitude"].as_f64().unwrap());
    close(
        place.longitude,
        expected_place["longitude"].as_f64().unwrap(),
    );

    let forecast = parse_open_meteo(&fixture("open-meteo-55115.json")).unwrap();
    let current = &contract["current"];
    assert_eq!(forecast.timezone, "America/Chicago");
    assert_eq!(forecast.current.time, current["time"].as_str().unwrap());
    close(
        forecast.current.temperature_c,
        current["temperature_c"].as_f64().unwrap(),
    );
    close(
        forecast.current.apparent_c,
        current["apparent_c"].as_f64().unwrap(),
    );
    assert_eq!(
        forecast.current.humidity,
        current["humidity"].as_i64().unwrap() as i32
    );
    close(
        forecast.current.wind_kmh,
        current["wind_kmh"].as_f64().unwrap(),
    );
    assert_eq!(
        forecast.current.weather_code,
        current["weather_code"].as_i64().unwrap() as i32
    );
    assert_eq!(
        forecast.hourly.len(),
        contract["hourly_count"].as_u64().unwrap() as usize
    );
    assert_eq!(
        forecast.daily.len(),
        contract["daily_count"].as_u64().unwrap() as usize
    );

    let info = describe(forecast.current.weather_code);
    assert_eq!(info.label, current["label"].as_str().unwrap());
    assert_eq!(info.icon, current["icon"].as_str().unwrap());
    assert_eq!(
        forecast.current.is_day,
        current["is_day"].as_bool().unwrap()
    );
    let icons = &contract["display_icons"];
    assert_eq!(
        display_icon("mainly-clear", true),
        icons["mainly_clear_day"].as_str().unwrap()
    );
    assert_eq!(
        display_icon("clear", false),
        icons["clear_night"].as_str().unwrap()
    );
    assert_eq!(
        display_icon("partly-cloudy", false),
        icons["partly_cloudy_night"].as_str().unwrap()
    );
    assert_eq!(
        display_icon("rain", false),
        icons["rain_night"].as_str().unwrap()
    );

    let hours = next_hours(&forecast, 24);
    assert_eq!(hours.len(), 24);
    assert_eq!(hours[0].time, contract["next_hour_time"].as_str().unwrap());
    assert_eq!(
        hours[0].precip_probability,
        Some(contract["next_hour0_pop"].as_i64().unwrap() as i32)
    );
    assert!(hours[0].is_day);
    assert!(!hours[1].is_day);
    let labels: Vec<String> = hours
        .iter()
        .take(9)
        .enumerate()
        .map(|(index, hour)| hour_label(&hour.time, index == 0))
        .collect();
    let expected_labels: Vec<&str> = contract["next_hour_labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(labels, expected_labels);
    assert_eq!(format_as_of(&forecast.current.time), "4:45 PM");

    let today = &forecast.current.time[..10];
    let day_labels: Vec<String> = forecast
        .daily
        .iter()
        .map(|day| day_label(&day.date, today))
        .collect();
    let expected_days: Vec<&str> = contract["day_labels"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(day_labels, expected_days);
    let daily0 = &contract["daily0"];
    close(
        forecast.daily[0].temp_max_c,
        daily0["max_c"].as_f64().unwrap(),
    );
    close(
        forecast.daily[0].temp_min_c,
        daily0["min_c"].as_f64().unwrap(),
    );
    assert_eq!(
        forecast.daily[0].precip_probability,
        daily0["pop"].as_i64().unwrap() as i32
    );
    assert_eq!(
        forecast.daily[0].weather_code,
        daily0["code"].as_i64().unwrap() as i32
    );
    close(
        forecast.daily[0].precip_sum_mm.unwrap(),
        daily0["precip_sum_mm"].as_f64().unwrap(),
    );
    close(
        forecast.daily[0].wind_max_kmh.unwrap(),
        daily0["wind_max_kmh"].as_f64().unwrap(),
    );
    assert_eq!(
        format_clock_time(forecast.daily[0].sunrise.as_deref().unwrap()),
        Some(daily0["sunrise"].as_str().unwrap().to_string())
    );
    assert_eq!(
        format_clock_time(forecast.daily[0].sunset.as_deref().unwrap()),
        Some(daily0["sunset"].as_str().unwrap().to_string())
    );
    assert_eq!(
        forecast.daily[0].humidity_mean,
        Some(daily0["humidity_mean"].as_i64().unwrap() as i32)
    );
    assert_eq!(
        forecast.daily[4].precip_probability,
        contract["daily4_pop"].as_i64().unwrap() as i32
    );

    let display = &contract["display"];
    assert_eq!(
        display_temperature(forecast.current.temperature_c, true),
        display["temp_f"].as_i64().unwrap() as i32
    );
    assert_eq!(
        display_temperature(forecast.current.apparent_c, true),
        display["apparent_f"].as_i64().unwrap() as i32
    );
    assert_eq!(
        display_temperature(forecast.current.temperature_c, false),
        display["temp_c"].as_i64().unwrap() as i32
    );
    assert_eq!(
        display_wind_mph(forecast.current.wind_kmh),
        display["wind_mph"].as_i64().unwrap() as i32
    );
    let rounding = &contract["rounding"];
    assert_eq!(
        round_half_away_from_zero(rounding["input"].as_f64().unwrap()),
        rounding["away_from_zero"].as_i64().unwrap() as i32
    );

    let chart = &contract["chart"];
    let values: Vec<f64> = chart["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_f64().unwrap())
        .collect();
    let points = chart_points(
        &values,
        chart["column_width"].as_f64().unwrap(),
        chart["height"].as_f64().unwrap(),
        chart["pad"].as_f64().unwrap(),
    );
    let expected_points = chart["points"].as_array().unwrap();
    assert_eq!(points.len(), expected_points.len());
    for (point, expected) in points.iter().zip(expected_points) {
        close(point.x, expected["x"].as_f64().unwrap());
        close(point.y, expected["y"].as_f64().unwrap());
    }
    let range = &chart["range"];
    let bar = range_placement(
        range["low"].as_f64().unwrap(),
        range["high"].as_f64().unwrap(),
        range["scale_min"].as_f64().unwrap(),
        range["scale_max"].as_f64().unwrap(),
    );
    close(bar.left, range["left"].as_f64().unwrap());
    close(bar.width, range["width"].as_f64().unwrap());
    let equal = &chart["equal_range"];
    let equal_bar = range_placement(
        equal["low"].as_f64().unwrap(),
        equal["high"].as_f64().unwrap(),
        equal["scale_min"].as_f64().unwrap(),
        equal["scale_max"].as_f64().unwrap(),
    );
    close(equal_bar.left, equal["left"].as_f64().unwrap());
    close(equal_bar.width, equal["width"].as_f64().unwrap());
}

#[test]
fn weather_codes_match_the_contract() {
    for row in contract()["codes"].as_array().unwrap() {
        let info = describe(row["code"].as_i64().unwrap() as i32);
        assert_eq!(info.label, row["label"].as_str().unwrap());
        assert_eq!(info.icon, row["icon"].as_str().unwrap());
    }
}

#[test]
fn alerts_parse_empty_and_a_wind_advisory() {
    let empty = parse_alerts(&fixture("nws-alerts-empty.json")).unwrap();
    assert!(empty.is_empty());
    let alerts = parse_alerts(&fixture("nws-alerts-wind.json")).unwrap();
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].event, "Wind Advisory");
    assert_eq!(alerts[0].severity, "Moderate");
    assert_eq!(alerts[0].sender_name, "NWS Twin Cities MN");
    assert!(alerts[0].instruction.contains("high profile"));
}

#[test]
fn radar_tiles_and_frames_match_the_contract() {
    let contract = contract();
    let tile = &contract["tile"];
    let point = lat_lon_to_tile(
        contract["place"]["latitude"].as_f64().unwrap(),
        contract["place"]["longitude"].as_f64().unwrap(),
        tile["zoom"].as_u64().unwrap() as u32,
    );
    assert_eq!(point.x, tile["x"].as_u64().unwrap() as u32);
    assert_eq!(point.y, tile["y"].as_u64().unwrap() as u32);
    assert_eq!(point.pixel_x, tile["pixel_x"].as_u64().unwrap() as u32);
    assert_eq!(point.pixel_y, tile["pixel_y"].as_u64().unwrap() as u32);
    assert_eq!(
        iem_tile_url("q2-hsr-900913", point.zoom, point.x, point.y),
        tile["url"].as_str().unwrap()
    );

    let window = &contract["window"];
    let grid = tile_window(
        contract["place"]["latitude"].as_f64().unwrap(),
        contract["place"]["longitude"].as_f64().unwrap(),
        window["zoom"].as_u64().unwrap() as u32,
        window["span"].as_u64().unwrap() as u32,
        window["span"].as_u64().unwrap() as u32,
    );
    assert_eq!(grid.origin_x, window["origin_x"].as_u64().unwrap() as u32);
    assert_eq!(grid.origin_y, window["origin_y"].as_u64().unwrap() as u32);
    assert_eq!(grid.pin_x, window["pin_x"].as_u64().unwrap() as u32);
    assert_eq!(grid.pin_y, window["pin_y"].as_u64().unwrap() as u32);

    let now: jiff::Timestamp = contract["radar_now"]
        .as_str()
        .unwrap()
        .parse()
        .expect("radar_now");
    let web = web_radar_frames_at(now);
    assert_eq!(web.len(), contract["web_frames"].as_u64().unwrap() as usize);
    assert_eq!(
        web[0].offset_minutes,
        contract["web_first"]["offset_minutes"].as_u64().unwrap() as u32
    );
    assert_eq!(
        web[0].layer,
        contract["web_first"]["layer"].as_str().unwrap()
    );
    assert_eq!(
        web[0].label,
        contract["web_first"]["label"].as_str().unwrap()
    );
    let last = web.last().unwrap();
    assert_eq!(
        last.offset_minutes,
        contract["web_last"]["offset_minutes"].as_u64().unwrap() as u32
    );
    assert_eq!(last.layer, contract["web_last"]["layer"].as_str().unwrap());
    assert_eq!(last.label, contract["web_last"]["label"].as_str().unwrap());

    let applet = applet_radar_frames_at(now);
    assert_eq!(
        applet.len(),
        contract["applet_frames"].as_u64().unwrap() as usize
    );
    assert_eq!(
        applet[0].offset_minutes,
        contract["applet_first_offset"].as_u64().unwrap() as u32
    );
    assert_eq!(applet.last().unwrap().label, "Latest scan");

    let lookback = &contract["radar_lookback"];
    let web_2h = web_radar_frames_for(RadarLookback::Hours2, now);
    assert_eq!(
        web_2h.len(),
        lookback["web_2h_frames"].as_u64().unwrap() as usize
    );
    assert_eq!(
        web_2h[0].offset_minutes,
        lookback["web_2h_first_offset"].as_u64().unwrap() as u32
    );
    let web_3h = web_radar_frames_for(RadarLookback::Hours3, now);
    assert_eq!(
        web_3h.len(),
        lookback["web_3h_frames"].as_u64().unwrap() as usize
    );
    assert_eq!(
        web_3h[0].offset_minutes,
        lookback["web_3h_first_offset"].as_u64().unwrap() as u32
    );
    let applet_2h = applet_radar_frames_for(RadarLookback::Hours2, now);
    assert_eq!(
        applet_2h.len(),
        lookback["applet_2h_frames"].as_u64().unwrap() as usize
    );
    assert_eq!(
        applet_2h[0].offset_minutes,
        lookback["applet_2h_first_offset"].as_u64().unwrap() as u32
    );
    let applet_3h = applet_radar_frames_for(RadarLookback::Hours3, now);
    assert_eq!(
        applet_3h.len(),
        lookback["applet_3h_frames"].as_u64().unwrap() as usize
    );
    assert_eq!(
        applet_3h[0].offset_minutes,
        lookback["applet_3h_first_offset"].as_u64().unwrap() as u32
    );

    let intervals = &contract["radar_intervals_ms"];
    assert_eq!(
        web_radar_interval_ms(RadarSpeed::X1),
        intervals["web_1x"].as_u64().unwrap() as u32
    );
    assert_eq!(
        web_radar_interval_ms(RadarSpeed::X2),
        intervals["web_2x"].as_u64().unwrap() as u32
    );
    assert_eq!(
        web_radar_interval_ms(RadarSpeed::X4),
        intervals["web_4x"].as_u64().unwrap() as u32
    );
    assert_eq!(
        applet_radar_interval_ms(RadarSpeed::X1),
        intervals["applet_1x"].as_u64().unwrap() as u32
    );
    assert_eq!(
        applet_radar_interval_ms(RadarSpeed::X2),
        intervals["applet_2x"].as_u64().unwrap() as u32
    );
    assert_eq!(
        applet_radar_interval_ms(RadarSpeed::X4),
        intervals["applet_4x"].as_u64().unwrap() as u32
    );
}

#[test]
fn relief_bands_and_ortho_match_the_contract() {
    let relief = &contract()["relief"];
    for band in relief["bands"].as_array().unwrap() {
        let height = band_height(
            band["r"].as_u64().unwrap() as u8,
            band["g"].as_u64().unwrap() as u8,
            band["b"].as_u64().unwrap() as u8,
        )
        .expect("band");
        close(f64::from(height), band["height"].as_f64().unwrap());
    }
    let cover = &relief["fit_cover"];
    let (half_w, half_h) = fit_ortho_half_extents(
        cover["half_w"].as_f64().unwrap() as f32,
        cover["half_h"].as_f64().unwrap() as f32,
        cover["aspect"].as_f64().unwrap() as f32,
        OrthoFit::Cover,
    );
    close(f64::from(half_w), cover["out_half_w"].as_f64().unwrap());
    close(f64::from(half_h), cover["out_half_h"].as_f64().unwrap());
    let contain = &relief["fit_contain"];
    let (half_w, half_h) = fit_ortho_half_extents(
        contain["half_w"].as_f64().unwrap() as f32,
        contain["half_h"].as_f64().unwrap() as f32,
        contain["aspect"].as_f64().unwrap() as f32,
        OrthoFit::Contain,
    );
    close(f64::from(half_w), contain["out_half_w"].as_f64().unwrap());
    close(f64::from(half_h), contain["out_half_h"].as_f64().unwrap());
}
