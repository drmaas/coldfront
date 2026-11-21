// SPDX-License-Identifier: GPL-3.0-only

//! Fetch basemap and MRMS radar tiles and composite them into one image.
//!
//! Tiles are cached by source, zoom, and coordinates so pan/zoom can recompose
//! without refetching. Radar frames load only when the Radar tab needs them.
//! Each IEM radar tile is denoised once at 256×256 and stored in the cache.

use std::collections::{HashMap, VecDeque};
use std::fmt;

use coldfront_core::{
    Alert, Error, Forecast, HttpClient, MapCamera, MapViewport, Place, RadarFrame, Raster,
    applet_radar_frames, compose_viewport, compose_viewport_ground, compose_viewport_radar_layer,
    denoise_radar_png, has_resolved_coordinates, iem_tile_url, is_us_zip, map_viewport,
    maptiler_tile_url, osm_tile_url, scale_raster, tile_camera,
};

/// Compact Radar tab image. Narrower than the popup so zoom controls sit on the map.
pub const MAP_VIEW_WIDTH: u32 = 400;
pub const MAP_VIEW_HEIGHT: u32 = 220;
const TILE_MARGIN: u32 = 1;
const DENOISE_CAP: usize = 512;

#[derive(Clone)]
pub enum BasemapKind {
    MapTiler(String),
    /// Opt-in only. The OSMF public tile servers are not a default app backend.
    Osm,
    None,
}

impl fmt::Debug for BasemapKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MapTiler(_) => write!(formatter, "MapTiler"),
            Self::Osm => write!(formatter, "Osm"),
            Self::None => write!(formatter, "None"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct WeatherBundle {
    pub place: Place,
    pub forecast: Forecast,
    pub alerts: Vec<Alert>,
}

#[derive(Clone, Debug)]
pub struct LoadedFrame {
    pub offset_minutes: u32,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    pub radar_tiles: usize,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum TileLayer {
    MapTiler,
    Osm,
    Radar { offset_minutes: u32 },
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct TileKey {
    layer: TileLayer,
    zoom: u32,
    x: u32,
    y: u32,
}

#[derive(Clone, Debug, Default)]
pub struct TileCache {
    tiles: HashMap<TileKey, Option<Vec<u8>>>,
    /// Insertion order for denoised radar tiles (LRU eviction at [`DENOISE_CAP`]).
    denoise_order: VecDeque<TileKey>,
}

impl TileCache {
    pub fn clear(&mut self) {
        self.tiles.clear();
        self.denoise_order.clear();
    }

    /// Keep successful tiles from `other` without dropping entries already cached.
    ///
    /// Stale fetch tasks still contribute edge tiles when their generation no
    /// longer matches, so pan does not lose work that already finished.
    pub fn absorb(&mut self, other: &TileCache) {
        for (key, value) in &other.tiles {
            if let Some(bytes) = value {
                let is_radar = matches!(key.layer, TileLayer::Radar { .. });
                let already = matches!(self.tiles.get(key), Some(Some(_)));
                self.tiles.insert(*key, Some(bytes.clone()));
                if is_radar && !already {
                    self.touch_denoise(*key);
                }
            }
        }
    }

    fn get(&self, key: TileKey) -> Option<&Option<Vec<u8>>> {
        self.tiles.get(&key)
    }

    fn insert_ok(&mut self, key: TileKey, bytes: Vec<u8>) {
        let is_radar = matches!(key.layer, TileLayer::Radar { .. });
        self.tiles.insert(key, Some(bytes));
        if is_radar {
            self.touch_denoise(key);
        }
    }

    fn touch_denoise(&mut self, key: TileKey) {
        self.denoise_order.retain(|item| *item != key);
        self.denoise_order.push_back(key);
        while self.denoise_order.len() > DENOISE_CAP {
            let Some(oldest) = self.denoise_order.pop_front() else {
                break;
            };
            self.tiles.remove(&oldest);
        }
    }

    /// True when the cache has no successful tile yet (absent or prior miss).
    fn needs_fetch(&self, key: TileKey) -> bool {
        !matches!(self.get(key), Some(Some(_)))
    }
}

pub async fn load_weather(client: &HttpClient, zip: &str) -> Result<WeatherBundle, Error> {
    let place = client.lookup_zip(zip).await?;
    load_weather_at_place(client, &place).await
}

pub async fn load_weather_at_place(
    client: &HttpClient,
    place: &Place,
) -> Result<WeatherBundle, Error> {
    let forecast = client.forecast(place.latitude, place.longitude).await?;
    let alerts = client
        .alerts(place.latitude, place.longitude)
        .await
        .unwrap_or_default();
    Ok(WeatherBundle {
        place: place.clone(),
        forecast,
        alerts,
    })
}

pub async fn load_weather_from_config(
    client: &HttpClient,
    zip: &str,
    city: &str,
    state_abbreviation: &str,
    latitude: f64,
    longitude: f64,
) -> Result<WeatherBundle, Error> {
    if is_us_zip(zip) {
        return load_weather(client, zip).await;
    }
    if has_resolved_coordinates(latitude, longitude) {
        let place = Place {
            zip: zip.to_owned(),
            city: city.to_owned(),
            state: String::new(),
            state_abbreviation: state_abbreviation.to_owned(),
            latitude,
            longitude,
        };
        return load_weather_at_place(client, &place).await;
    }
    Err(Error::InvalidZip)
}

#[must_use]
pub fn viewport_for(
    latitude: f64,
    longitude: f64,
    zoom: f64,
    pan_x: f64,
    pan_y: f64,
    view_width: u32,
    view_height: u32,
) -> (MapViewport, f64) {
    let tile = tile_camera(zoom, pan_x, pan_y, view_width, view_height);
    let viewport = map_viewport(MapCamera {
        latitude,
        longitude,
        zoom: tile.zoom,
        pan_x: tile.pan_x,
        pan_y: tile.pan_y,
        view_width: tile.view_width,
        view_height: tile.view_height,
        margin: TILE_MARGIN,
    });
    (viewport, tile.scale)
}

/// Place, camera, and frame needed to fill and compose a map.
pub struct MapRequest {
    pub latitude: f64,
    pub longitude: f64,
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
    pub view_width: u32,
    pub view_height: u32,
    /// Device pixels per logical pixel, capped at 2 by the caller.
    pub pixel_ratio: f64,
    pub offset_minutes: u32,
}

/// Present-time scale from an integer tile viewport to the logical map size.
#[derive(Clone, Copy, Debug)]
pub struct PresentScale {
    pub frac_scale: f64,
    pub view_width: u32,
    pub view_height: u32,
    pub pixel_ratio: f64,
}

/// Fetch any tiles missing from the cache for the current viewport and frame,
/// then compose the visible raster. Failed fetches stay absent so the next
/// settle can retry; holes paint as transparent / pale background.
pub async fn ensure_map_frame(
    client: &HttpClient,
    cache: &mut TileCache,
    kind: &BasemapKind,
    request: &MapRequest,
) -> Result<LoadedFrame, Error> {
    ensure_map_frames(client, cache, kind, request, &[]).await
}

/// Fetch tiles for `request` plus any extra radar offsets at the same camera,
/// then compose the shown frame. Extra offsets fill the cache for playback
/// after pan/zoom without restarting the whole load pipeline.
pub async fn ensure_map_frames(
    client: &HttpClient,
    cache: &mut TileCache,
    kind: &BasemapKind,
    request: &MapRequest,
    extra_offsets: &[u32],
) -> Result<LoadedFrame, Error> {
    let (viewport, scale) = viewport_for(
        request.latitude,
        request.longitude,
        request.zoom,
        request.pan_x,
        request.pan_y,
        request.view_width,
        request.view_height,
    );
    let frames = applet_radar_frames();
    let primary = frames
        .iter()
        .find(|item| item.offset_minutes == request.offset_minutes)
        .ok_or_else(|| Error::Parse("unknown radar frame".into()))?;
    fill_viewport_tiles(client, cache, kind, &viewport, primary).await;
    for offset in extra_offsets {
        if *offset == request.offset_minutes {
            continue;
        }
        let Some(frame) = frames.iter().find(|item| item.offset_minutes == *offset) else {
            continue;
        };
        fill_viewport_tiles(client, cache, kind, &viewport, frame).await;
    }
    compose_cached(
        cache,
        kind,
        &viewport,
        PresentScale {
            frac_scale: scale,
            view_width: request.view_width,
            view_height: request.view_height,
            pixel_ratio: request.pixel_ratio,
        },
        request.offset_minutes,
    )
}

/// Warm tiles for neighbor integer zooms and an optional next radar offset.
///
/// Does not composite. Used after a successful settle so the next pan/zoom
/// already has edge tiles.
pub async fn prefetch_around(
    client: &HttpClient,
    cache: &mut TileCache,
    kind: &BasemapKind,
    request: &MapRequest,
    next_offset: Option<u32>,
) {
    let floor = request.zoom.floor() as u32;
    let warm = |zoom: f64, offset: u32| {
        let (viewport, _) = viewport_for(
            request.latitude,
            request.longitude,
            zoom,
            request.pan_x,
            request.pan_y,
            request.view_width,
            request.view_height,
        );
        let frames = applet_radar_frames();
        frames
            .into_iter()
            .find(|frame| frame.offset_minutes == offset)
            .map(|frame| (viewport, frame))
    };
    if floor > coldfront_core::APPLET_ZOOM_MIN
        && let Some((viewport, frame)) = warm(f64::from(floor - 1), request.offset_minutes)
    {
        fill_viewport_tiles(client, cache, kind, &viewport, &frame).await;
    }
    if floor < coldfront_core::APPLET_ZOOM_MAX
        && let Some((viewport, frame)) = warm(f64::from(floor + 1), request.offset_minutes)
    {
        fill_viewport_tiles(client, cache, kind, &viewport, &frame).await;
    }
    if let Some(offset) = next_offset
        && offset != request.offset_minutes
        && let Some((viewport, frame)) = warm(request.zoom, offset)
    {
        fill_viewport_tiles(client, cache, kind, &viewport, &frame).await;
    }
}

/// Recompose from cache for the current camera.
///
/// Missing tiles paint as holes (pale basemap background / transparent radar),
/// matching the web canvas path that draws only tiles already loaded.
pub fn compose_cached(
    cache: &TileCache,
    kind: &BasemapKind,
    viewport: &MapViewport,
    present: PresentScale,
    offset_minutes: u32,
) -> Result<LoadedFrame, Error> {
    let basemap_layer = match kind {
        BasemapKind::MapTiler(_) => Some(TileLayer::MapTiler),
        BasemapKind::Osm => Some(TileLayer::Osm),
        BasemapKind::None => None,
    };
    let radar_layer = TileLayer::Radar { offset_minutes };
    let coords = viewport.tile_coords();
    let mut basemap = Vec::with_capacity(coords.len());
    let mut radar = Vec::with_capacity(coords.len());
    let mut radar_tiles = 0usize;
    for &(x, y) in &coords {
        if let Some(layer) = basemap_layer {
            let key = TileKey {
                layer,
                zoom: viewport.zoom,
                x,
                y,
            };
            match cache.get(key) {
                None | Some(None) => basemap.push(None),
                Some(Some(bytes)) => basemap.push(Some(bytes.clone())),
            }
        } else {
            basemap.push(None);
        }
        let key = TileKey {
            layer: radar_layer,
            zoom: viewport.zoom,
            x,
            y,
        };
        match cache.get(key) {
            None | Some(None) => radar.push(None),
            Some(Some(bytes)) => {
                radar_tiles += 1;
                radar.push(Some(bytes.clone()));
            }
        }
    }
    let raster = compose_viewport(viewport, &basemap, &radar)?;
    let out = present_raster(&raster, present);
    Ok(LoadedFrame {
        offset_minutes,
        width: out.width,
        height: out.height,
        pixels: out.pixels,
        radar_tiles,
    })
}

/// Basemap (or pale fill) plus pin, and a radar-only layer for Relief sampling.
pub fn compose_relief_layers(
    cache: &TileCache,
    kind: &BasemapKind,
    viewport: &MapViewport,
    present: PresentScale,
    offset_minutes: u32,
) -> Result<(Raster, Raster, usize), Error> {
    let basemap_layer = match kind {
        BasemapKind::MapTiler(_) => Some(TileLayer::MapTiler),
        BasemapKind::Osm => Some(TileLayer::Osm),
        BasemapKind::None => None,
    };
    let radar_layer = TileLayer::Radar { offset_minutes };
    let coords = viewport.tile_coords();
    let mut basemap = Vec::with_capacity(coords.len());
    let mut radar = Vec::with_capacity(coords.len());
    let mut radar_tiles = 0usize;
    for &(x, y) in &coords {
        if let Some(layer) = basemap_layer {
            let key = TileKey {
                layer,
                zoom: viewport.zoom,
                x,
                y,
            };
            match cache.get(key) {
                None | Some(None) => basemap.push(None),
                Some(Some(bytes)) => basemap.push(Some(bytes.clone())),
            }
        } else {
            basemap.push(None);
        }
        let key = TileKey {
            layer: radar_layer,
            zoom: viewport.zoom,
            x,
            y,
        };
        match cache.get(key) {
            None | Some(None) => radar.push(None),
            Some(Some(bytes)) => {
                radar_tiles += 1;
                radar.push(Some(bytes.clone()));
            }
        }
    }
    let ground = compose_viewport_ground(viewport, &basemap)?;
    let radar_layer = compose_viewport_radar_layer(viewport, &radar)?;
    let ground = present_raster(&ground, present);
    let radar_layer = present_raster(&radar_layer, present);
    Ok((ground, radar_layer, radar_tiles))
}

fn present_raster(raster: &Raster, present: PresentScale) -> Raster {
    let ratio = present.pixel_ratio.clamp(1.0, 2.0);
    let out_w = ((f64::from(present.view_width.max(1)) * ratio).round() as u32).max(1);
    let out_h = ((f64::from(present.view_height.max(1)) * ratio).round() as u32).max(1);
    // Integer fractional zoom: nearest. Fractional zoom: smooth the scaled tiles.
    if (present.frac_scale - 1.0).abs() < 0.001
        && (ratio - 1.0).abs() < 0.001
        && raster.width == out_w
        && raster.height == out_h
    {
        return raster.clone();
    }
    scale_raster(raster, out_w, out_h)
}

async fn fill_viewport_tiles(
    client: &HttpClient,
    cache: &mut TileCache,
    kind: &BasemapKind,
    viewport: &MapViewport,
    frame: &RadarFrame,
) {
    let mut missing: Vec<(TileKey, String, bool)> = Vec::new();
    for (x, y) in viewport.tile_coords() {
        match kind {
            BasemapKind::MapTiler(key) => {
                let tile_key = TileKey {
                    layer: TileLayer::MapTiler,
                    zoom: viewport.zoom,
                    x,
                    y,
                };
                if cache.needs_fetch(tile_key) {
                    missing.push((tile_key, maptiler_tile_url(viewport.zoom, x, y, key), false));
                }
            }
            BasemapKind::Osm => {
                let tile_key = TileKey {
                    layer: TileLayer::Osm,
                    zoom: viewport.zoom,
                    x,
                    y,
                };
                if cache.needs_fetch(tile_key) {
                    missing.push((tile_key, osm_tile_url(viewport.zoom, x, y), false));
                }
            }
            BasemapKind::None => {}
        }
        let radar_key = TileKey {
            layer: TileLayer::Radar {
                offset_minutes: frame.offset_minutes,
            },
            zoom: viewport.zoom,
            x,
            y,
        };
        if cache.needs_fetch(radar_key) {
            missing.push((
                radar_key,
                iem_tile_url(&frame.layer, viewport.zoom, x, y),
                true,
            ));
        }
    }
    if missing.is_empty() {
        return;
    }
    let downloads = missing.into_iter().map(|(key, url, denoise)| {
        let client = client.clone();
        async move { (key, client.get_bytes(&url).await.ok(), denoise) }
    });
    for (key, bytes, denoise) in futures::future::join_all(downloads).await {
        // Leave failures absent so the next settle can retry.
        let Some(bytes) = bytes else {
            continue;
        };
        let stored = if denoise {
            denoise_radar_png(&bytes).unwrap_or(bytes)
        } else {
            bytes
        };
        cache.insert_ok(key, stored);
    }
}
