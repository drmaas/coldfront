// SPDX-License-Identifier: GPL-3.0-only

use crate::alerts::Alert;
use crate::error::Error;
use crate::forecast::Forecast;
use crate::geocode::{GeocodeHit, geocode_url, parse_geocode_search};
use crate::place::Place;
use crate::zipcode::is_us_zip;

/// Sent on every upstream request. NWS rejects clients that omit one.
pub const USER_AGENT: &str = "Coldfront/0.1 (https://github.com/drmaas/coldfront)";

/// Open-Meteo forecast request. The query must match `OPEN_METEO` in
/// `apps/coldfront-web/lib/server/proxy.ts`. `fixtures/contract.json` locks it.
pub fn forecast_url(latitude: f64, longitude: f64) -> String {
    format!(
        "https://api.open-meteo.com/v1/forecast?latitude={latitude}&longitude={longitude}\
         &current=temperature_2m,relative_humidity_2m,apparent_temperature,weather_code,wind_speed_10m,is_day\
         &hourly=temperature_2m,weather_code,precipitation_probability,is_day\
         &daily=weather_code,temperature_2m_max,temperature_2m_min,precipitation_probability_max,precipitation_sum,wind_speed_10m_max,relative_humidity_2m_mean,sunrise,sunset\
         &temperature_unit=celsius&wind_speed_unit=kmh&timezone=auto&forecast_days=10"
    )
}

/// HTTP client for the weather services both apps use.
#[derive(Clone)]
pub struct HttpClient {
    http: reqwest::Client,
}

impl HttpClient {
    pub fn new() -> Result<Self, Error> {
        let http = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(12))
            .build()
            .map_err(|_| Error::Http("could not build the http client".into()))?;
        Ok(Self { http })
    }

    /// Fetch bytes. Errors omit the URL so a MapTiler key in a tile URL is not logged.
    pub async fn get_bytes(&self, url: &str) -> Result<Vec<u8>, Error> {
        let response = self
            .http
            .get(url)
            .send()
            .await
            .map_err(|_| Error::Http("request failed".into()))?;
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(Error::NotFound);
        }
        if !status.is_success() {
            return Err(Error::Upstream(status.as_u16()));
        }
        let bytes = response
            .bytes()
            .await
            .map_err(|_| Error::Http("could not read the response".into()))?;
        Ok(bytes.to_vec())
    }

    pub async fn lookup_zip(&self, zip: &str) -> Result<Place, Error> {
        if !is_us_zip(zip) {
            return Err(Error::InvalidZip);
        }
        let bytes = self
            .get_bytes(&format!("https://api.zippopotam.us/us/{zip}"))
            .await?;
        crate::place::parse_zippopotam(&bytes)
    }

    pub async fn search_cities(&self, query: &str) -> Result<Vec<GeocodeHit>, Error> {
        let trimmed = query.trim();
        if trimmed.is_empty() || !crate::geocode::geocode_query_allowed(trimmed) {
            return Ok(Vec::new());
        }
        let bytes = self.get_bytes(&geocode_url(trimmed, 8)).await?;
        parse_geocode_search(&bytes)
    }

    pub async fn forecast(&self, latitude: f64, longitude: f64) -> Result<Forecast, Error> {
        let bytes = self.get_bytes(&forecast_url(latitude, longitude)).await?;
        crate::forecast::parse_open_meteo(&bytes)
    }

    pub async fn alerts(&self, latitude: f64, longitude: f64) -> Result<Vec<Alert>, Error> {
        let url = format!("https://api.weather.gov/alerts/active?point={latitude},{longitude}");
        let bytes = self.get_bytes(&url).await?;
        crate::alerts::parse_alerts(&bytes)
    }
}
