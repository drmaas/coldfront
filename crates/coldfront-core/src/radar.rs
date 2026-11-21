// SPDX-License-Identifier: GPL-3.0-only

//! Iowa State Mesonet NEXRAD tile URLs and a small raster compositor.
//!
//! libcosmic has no map widget. The applet fetches these tiles and draws the
//! composited image. The web app uses the same URL and frame list.

use crate::error::Error;
use crate::units::round_half_away_from_zero;
use image::{Rgba, RgbaImage, imageops};
use jiff::{Timestamp, ToSpan};

pub const IEM_TILE_ORIGIN: &str = "https://mesonet.agron.iastate.edu/cache/tile.py/1.0.0";
pub const MAP_DISPLAY_WIDTH: u32 = 400;
pub const MAP_DISPLAY_HEIGHT: u32 = 260;
pub const APPLET_ZOOM_MIN: u32 = 5;
pub const APPLET_ZOOM_MAX: u32 = 11;
pub const TILE_SIZE: u32 = 256;

/// One radar animation frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RadarFrame {
    pub offset_minutes: u32,
    pub layer: String,
    pub label: String,
}

/// Web Mercator tile containing a latitude and longitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TilePoint {
    pub zoom: u32,
    pub x: u32,
    pub y: u32,
    pub pixel_x: u32,
    pub pixel_y: u32,
}

/// A tile grid centered near a point, in tile coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileWindow {
    pub zoom: u32,
    pub origin_x: u32,
    pub origin_y: u32,
    pub span_x: u32,
    pub span_y: u32,
    /// Pin position in pixels of the uncropped grid, from the top left.
    pub pin_x: u32,
    pub pin_y: u32,
}

/// RGBA image ready for an iced `Handle::from_rgba`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Tile grid and crop for an interactive map view (pan/zoom).
///
/// The grid includes a margin of tiles around the visible area so edge tiles
/// are ready while the user drags. `crop_x`/`crop_y` are the top-left of the
/// visible view inside that grid, in pixels. The pin is in view pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapViewport {
    pub zoom: u32,
    pub origin_x: u32,
    pub origin_y: u32,
    pub span_x: u32,
    pub span_y: u32,
    pub crop_x: u32,
    pub crop_y: u32,
    pub view_width: u32,
    pub view_height: u32,
    pub pin_x: i32,
    pub pin_y: i32,
}

impl MapViewport {
    /// Convert to a [`TileWindow`] for loaders that still take that type.
    #[must_use]
    pub fn as_tile_window(&self) -> TileWindow {
        TileWindow {
            zoom: self.zoom,
            origin_x: self.origin_x,
            origin_y: self.origin_y,
            span_x: self.span_x,
            span_y: self.span_y,
            pin_x: 0,
            pin_y: 0,
        }
    }

    /// Absolute tile coordinates in row-major order matching a tile slice.
    #[must_use]
    pub fn tile_coords(&self) -> Vec<(u32, u32)> {
        let mut coords = Vec::with_capacity((self.span_x * self.span_y) as usize);
        for row in 0..self.span_y {
            for col in 0..self.span_x {
                coords.push((self.origin_x + col, self.origin_y + row));
            }
        }
        coords
    }
}

/// Twelve frames, every five minutes from 55 minutes ago through the latest scan.
#[must_use]
pub fn web_radar_frames() -> Vec<RadarFrame> {
    web_radar_frames_at(Timestamp::now())
}

/// Same as [`web_radar_frames`], with a fixed `now` for tests.
#[must_use]
pub fn web_radar_frames_at(now: Timestamp) -> Vec<RadarFrame> {
    (0..=11).rev().map(|step| frame(step * 5, now)).collect()
}

/// Six frames, every ten minutes. Fewer tiles than the web loop, so the popup
/// can show the latest scan quickly and then animate.
#[must_use]
pub fn applet_radar_frames() -> Vec<RadarFrame> {
    applet_radar_frames_at(Timestamp::now())
}

/// Same as [`applet_radar_frames`], with a fixed `now` for tests.
#[must_use]
pub fn applet_radar_frames_at(now: Timestamp) -> Vec<RadarFrame> {
    [50, 40, 30, 20, 10, 0]
        .into_iter()
        .map(|offset| frame(offset, now))
        .collect()
}

/// IEM tile layer for one animation offset.
///
/// Latest uses MRMS hybrid-scan reflectivity (`q2-hsr`), which drops most
/// ground clutter that NEXRAD base reflectivity keeps. Older frames use the
/// MRMS SeamlessHSR archive (`mrms::lcref-{UTC}`), floored to an even minute.
#[must_use]
pub fn radar_layer(offset_minutes: u32, now: Timestamp) -> String {
    if offset_minutes == 0 {
        return "q2-hsr-900913".to_string();
    }
    let past = now
        .checked_sub(i64::from(offset_minutes).minutes())
        .unwrap_or(now);
    let secs = past.as_second();
    let floored = secs - secs.rem_euclid(120);
    let stamp = Timestamp::from_second(floored)
        .unwrap_or(past)
        .strftime("%Y%m%d%H%M")
        .to_string();
    format!("mrms::lcref-{stamp}")
}

#[must_use]
pub fn iem_tile_url(layer: &str, zoom: u32, x: u32, y: u32) -> String {
    format!("{IEM_TILE_ORIGIN}/{layer}/{zoom}/{x}/{y}.png")
}

#[must_use]
pub fn maptiler_tile_url(zoom: u32, x: u32, y: u32, key: &str) -> String {
    format!("https://api.maptiler.com/maps/basic-v2/{zoom}/{x}/{y}.png?key={key}")
}

/// Public OSM raster tiles. The applet only requests these when the user opts in.
/// The OSMF tile usage policy forbids shipping an app that hits this server by default.
#[must_use]
pub fn osm_tile_url(zoom: u32, x: u32, y: u32) -> String {
    format!("https://tile.openstreetmap.org/{zoom}/{x}/{y}.png")
}

#[must_use]
pub fn lat_lon_to_tile(latitude: f64, longitude: f64, zoom: u32) -> TilePoint {
    let (x, y) = world(latitude, longitude, zoom);
    let tile_x = x.floor().max(0.0) as u32;
    let tile_y = y.floor().max(0.0) as u32;
    TilePoint {
        zoom,
        x: tile_x,
        y: tile_y,
        pixel_x: clamp_pixel((x - f64::from(tile_x)) * f64::from(TILE_SIZE)),
        pixel_y: clamp_pixel((y - f64::from(tile_y)) * f64::from(TILE_SIZE)),
    }
}

#[must_use]
pub fn tile_window(
    latitude: f64,
    longitude: f64,
    zoom: u32,
    span_x: u32,
    span_y: u32,
) -> TileWindow {
    let (x, y) = world(latitude, longitude, zoom);
    let limit = (1i32 << zoom) - span_x as i32;
    let limit_y = (1i32 << zoom) - span_y as i32;
    let origin_x =
        round_half_away_from_zero(x - f64::from(span_x) / 2.0).clamp(0, limit.max(0)) as u32;
    let origin_y =
        round_half_away_from_zero(y - f64::from(span_y) / 2.0).clamp(0, limit_y.max(0)) as u32;
    let pin_x =
        round_half_away_from_zero((x - f64::from(origin_x)) * f64::from(TILE_SIZE)).max(0) as u32;
    let pin_y =
        round_half_away_from_zero((y - f64::from(origin_y)) * f64::from(TILE_SIZE)).max(0) as u32;
    TileWindow {
        zoom,
        origin_x,
        origin_y,
        span_x,
        span_y,
        pin_x,
        pin_y,
    }
}

/// Center, zoom, pan, and size for [`map_viewport`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapCamera {
    pub latitude: f64,
    pub longitude: f64,
    pub zoom: u32,
    pub pan_x: f64,
    pub pan_y: f64,
    pub view_width: u32,
    pub view_height: u32,
    pub margin: u32,
}

/// Continuous camera used while the applet pans and zooms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GestureCamera {
    pub zoom: f64,
    pub pan_x: f64,
    pub pan_y: f64,
}

/// Affine map from a settled radar bitmap to the live camera.
///
/// `sx_live = scale * sx_settled + tx` in CSS / logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SnapshotTransform {
    pub scale: f64,
    pub tx: f64,
    pub ty: f64,
}

/// Integer tile camera derived from a continuous gesture camera.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileCamera {
    pub zoom: u32,
    pub pan_x: f64,
    pub pan_y: f64,
    pub view_width: u32,
    pub view_height: u32,
    /// Scale from tile pixels to logical view pixels: `2^(zoom - floor(zoom))`.
    pub scale: f64,
}

/// Affine map from a settled radar bitmap to the live camera.
#[must_use]
pub fn snapshot_transform(
    settled: GestureCamera,
    live: GestureCamera,
    width: f64,
    height: f64,
) -> SnapshotTransform {
    let scale = 2f64.powf(live.zoom - settled.zoom);
    SnapshotTransform {
        scale,
        tx: width / 2.0 + live.pan_x - scale * (width / 2.0 + settled.pan_x),
        ty: height / 2.0 + live.pan_y - scale * (height / 2.0 + settled.pan_y),
    }
}

/// Zoom toward a screen origin, keeping that point fixed in the view.
#[must_use]
pub fn zoom_toward(
    camera: GestureCamera,
    next_zoom: f64,
    origin_x: f64,
    origin_y: f64,
    width: f64,
    height: f64,
) -> GestureCamera {
    let zoom = next_zoom.clamp(f64::from(APPLET_ZOOM_MIN), f64::from(APPLET_ZOOM_MAX));
    if (zoom - camera.zoom).abs() < f64::EPSILON {
        return GestureCamera {
            zoom,
            pan_x: camera.pan_x,
            pan_y: camera.pan_y,
        };
    }
    let factor = 2f64.powf(zoom - camera.zoom);
    GestureCamera {
        zoom,
        pan_x: factor * (width / 2.0 + camera.pan_x - origin_x) - width / 2.0 + origin_x,
        pan_y: factor * (height / 2.0 + camera.pan_y - origin_y) - height / 2.0 + origin_y,
    }
}

/// Map a continuous zoom onto the integer [`map_viewport`] camera.
#[must_use]
pub fn tile_camera(
    zoom: f64,
    pan_x: f64,
    pan_y: f64,
    view_width: u32,
    view_height: u32,
) -> TileCamera {
    let zoom = zoom.clamp(f64::from(APPLET_ZOOM_MIN), f64::from(APPLET_ZOOM_MAX));
    let tile_zoom = zoom.floor() as u32;
    let tile_zoom = tile_zoom.clamp(APPLET_ZOOM_MIN, APPLET_ZOOM_MAX);
    let scale = 2f64.powf(zoom - f64::from(tile_zoom));
    let view_width = ((f64::from(view_width.max(1)) / scale).round() as u32).max(1);
    let view_height = ((f64::from(view_height.max(1)) / scale).round() as u32).max(1);
    TileCamera {
        zoom: tile_zoom,
        pan_x: pan_x / scale,
        pan_y: pan_y / scale,
        view_width,
        view_height,
        scale,
    }
}

/// Mercator unit coordinates for a latitude and longitude (zoom-independent).
#[must_use]
pub fn pin_mercator(latitude: f64, longitude: f64) -> (f64, f64) {
    let mx = (longitude + 180.0) / 360.0;
    let lat_rad = latitude.to_radians();
    let my = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0;
    (mx, my)
}

/// Screen point for a mercator location under a continuous camera.
#[must_use]
pub fn world_to_screen(
    latitude: f64,
    longitude: f64,
    camera: GestureCamera,
    width: f64,
    height: f64,
    mx: f64,
    my: f64,
) -> (f64, f64) {
    let (pin_x, pin_y) = pin_mercator(latitude, longitude);
    (
        (mx - pin_x) * f64::from(TILE_SIZE) * 2f64.powf(camera.zoom) + width / 2.0 + camera.pan_x,
        (my - pin_y) * f64::from(TILE_SIZE) * 2f64.powf(camera.zoom) + height / 2.0 + camera.pan_y,
    )
}

/// Resize a raster to `width` by `height`.
///
/// Scale near 1.0 uses nearest neighbor so integer zooms stay sharp.
#[must_use]
pub fn scale_raster(source: &Raster, width: u32, height: u32) -> Raster {
    let width = width.max(1);
    let height = height.max(1);
    if source.width == width && source.height == height {
        return source.clone();
    }
    let Some(image) = RgbaImage::from_raw(source.width, source.height, source.pixels.clone())
    else {
        return Raster {
            width,
            height,
            pixels: vec![0; (width * height * 4) as usize],
        };
    };
    let sx = f64::from(width) / f64::from(source.width.max(1));
    let sy = f64::from(height) / f64::from(source.height.max(1));
    let integer_scale = (sx - sx.round()).abs() < 0.001
        && (sy - sy.round()).abs() < 0.001
        && sx >= 1.0
        && sy >= 1.0;
    let filter = if integer_scale {
        imageops::FilterType::Nearest
    } else {
        imageops::FilterType::Triangle
    };
    let resized = imageops::resize(&image, width, height, filter);
    Raster {
        width: resized.width(),
        height: resized.height(),
        pixels: resized.into_raw(),
    }
}

/// Tile grid covering a panned/zoomed view, with a one-tile margin by default.
///
/// `pan_x`/`pan_y` are screen pixels: positive values move the map content
/// right/down, matching the web `RadarMap` canvas.
#[must_use]
pub fn map_viewport(camera: MapCamera) -> MapViewport {
    let zoom = camera.zoom.clamp(APPLET_ZOOM_MIN, APPLET_ZOOM_MAX);
    let view_width = camera.view_width.max(1);
    let view_height = camera.view_height.max(1);
    let (cx, cy) = world(camera.latitude, camera.longitude, zoom);
    let left = cx
        - f64::from(view_width) / 2.0 / f64::from(TILE_SIZE)
        - camera.pan_x / f64::from(TILE_SIZE);
    let top = cy
        - f64::from(view_height) / 2.0 / f64::from(TILE_SIZE)
        - camera.pan_y / f64::from(TILE_SIZE);
    let limit = 1u32 << zoom;
    let margin = camera.margin as i32;
    let x0 = (left.floor() as i32 - margin).max(0) as u32;
    let y0 = (top.floor() as i32 - margin).max(0) as u32;
    let x1 = ((left + f64::from(view_width) / f64::from(TILE_SIZE)).floor() as i32 + margin)
        .clamp(0, limit.saturating_sub(1) as i32) as u32;
    let y1 = ((top + f64::from(view_height) / f64::from(TILE_SIZE)).floor() as i32 + margin)
        .clamp(0, limit.saturating_sub(1) as i32) as u32;
    let origin_x = x0.min(limit.saturating_sub(1));
    let origin_y = y0.min(limit.saturating_sub(1));
    let span_x = x1.saturating_sub(origin_x).saturating_add(1).max(1);
    let span_y = y1.saturating_sub(origin_y).saturating_add(1).max(1);
    let crop_x = round_half_away_from_zero((left - f64::from(origin_x)) * f64::from(TILE_SIZE))
        .max(0) as u32;
    let crop_y =
        round_half_away_from_zero((top - f64::from(origin_y)) * f64::from(TILE_SIZE)).max(0) as u32;
    let pin_x = round_half_away_from_zero(f64::from(view_width) / 2.0 + camera.pan_x);
    let pin_y = round_half_away_from_zero(f64::from(view_height) / 2.0 + camera.pan_y);
    MapViewport {
        zoom,
        origin_x,
        origin_y,
        span_x,
        span_y,
        crop_x,
        crop_y,
        view_width,
        view_height,
        pin_x,
        pin_y,
    }
}

/// Stack basemap and radar tiles for an interactive viewport and draw the pin.
///
/// Missing tiles leave the pale background (basemap) or stay transparent (radar).
/// Output size matches `viewport.view_width` by `viewport.view_height`.
pub fn compose_viewport(
    viewport: &MapViewport,
    basemap: &[Option<Vec<u8>>],
    radar: &[Option<Vec<u8>>],
) -> Result<Raster, Error> {
    let mut output =
        compose_viewport_layers(viewport, basemap, radar, Rgba([213, 227, 236, 255]), true)?;
    draw_pin(&mut output, viewport.pin_x, viewport.pin_y);
    Ok(Raster {
        width: output.width(),
        height: output.height(),
        pixels: output.into_raw(),
    })
}

/// Basemap (or pale fill) plus the ZIP pin. No radar underlay for Relief.
pub fn compose_viewport_ground(
    viewport: &MapViewport,
    basemap: &[Option<Vec<u8>>],
) -> Result<Raster, Error> {
    let empty: Vec<Option<Vec<u8>>> = vec![None; basemap.len().max(1)];
    let mut output =
        compose_viewport_layers(viewport, basemap, &empty, Rgba([213, 227, 236, 255]), true)?;
    draw_pin(&mut output, viewport.pin_x, viewport.pin_y);
    Ok(Raster {
        width: output.width(),
        height: output.height(),
        pixels: output.into_raw(),
    })
}

/// Radar tiles only on a transparent clear. Same crop and size as the ground.
pub fn compose_viewport_radar_layer(
    viewport: &MapViewport,
    radar: &[Option<Vec<u8>>],
) -> Result<Raster, Error> {
    let empty: Vec<Option<Vec<u8>>> = vec![None; radar.len().max(1)];
    let output = compose_viewport_layers(viewport, &empty, radar, Rgba([0, 0, 0, 0]), false)?;
    Ok(Raster {
        width: output.width(),
        height: output.height(),
        pixels: output.into_raw(),
    })
}

fn compose_viewport_layers(
    viewport: &MapViewport,
    basemap: &[Option<Vec<u8>>],
    radar: &[Option<Vec<u8>>],
    clear: Rgba<u8>,
    blit_basemap: bool,
) -> Result<RgbaImage, Error> {
    let grid = viewport.as_tile_window();
    let width = grid.span_x.saturating_mul(TILE_SIZE);
    let height = grid.span_y.saturating_mul(TILE_SIZE);
    if width == 0 || height == 0 || viewport.view_width == 0 || viewport.view_height == 0 {
        return Err(Error::Parse("map viewport is empty".into()));
    }
    let mut image = RgbaImage::from_pixel(width, height, clear);
    if blit_basemap {
        blit_tiles(&mut image, &grid, basemap);
    }
    blit_tiles(&mut image, &grid, radar);

    let crop_w = viewport.view_width.min(width);
    let crop_h = viewport.view_height.min(height);
    let max_left = width.saturating_sub(crop_w);
    let max_top = height.saturating_sub(crop_h);
    let left = viewport.crop_x.min(max_left);
    let top = viewport.crop_y.min(max_top);
    let cropped = imageops::crop(&mut image, left, top, crop_w, crop_h).to_image();
    if crop_w == viewport.view_width && crop_h == viewport.view_height {
        Ok(cropped)
    } else {
        Ok(imageops::resize(
            &cropped,
            viewport.view_width,
            viewport.view_height,
            imageops::FilterType::Triangle,
        ))
    }
}

/// Stack basemap tiles, radar tiles, crop around the pin, and draw the pin.
///
/// Missing tiles leave the pale background (basemap) or stay transparent (radar).
pub fn compose_radar_map(
    grid: &TileWindow,
    basemap: &[Option<Vec<u8>>],
    radar: &[Option<Vec<u8>>],
) -> Result<Raster, Error> {
    let width = grid.span_x.saturating_mul(TILE_SIZE);
    let height = grid.span_y.saturating_mul(TILE_SIZE);
    if width == 0 || height == 0 {
        return Err(Error::Parse("map grid is empty".into()));
    }
    let mut image = RgbaImage::from_pixel(width, height, Rgba([213, 227, 236, 255]));
    blit_tiles(&mut image, grid, basemap);
    blit_tiles(&mut image, grid, radar);

    let crop_w = 512.min(width);
    let crop_h = 340.min(height);
    let pin_x = grid.pin_x.min(width.saturating_sub(1));
    let pin_y = grid.pin_y.min(height.saturating_sub(1));
    let left = (pin_x as i32 - crop_w as i32 / 2).clamp(0, (width - crop_w) as i32) as u32;
    let top = (pin_y as i32 - crop_h as i32 / 2).clamp(0, (height - crop_h) as i32) as u32;
    let cropped = imageops::crop(&mut image, left, top, crop_w, crop_h).to_image();
    let local_x = pin_x as i32 - left as i32;
    let local_y = pin_y as i32 - top as i32;
    let mut output = imageops::resize(
        &cropped,
        MAP_DISPLAY_WIDTH,
        MAP_DISPLAY_HEIGHT,
        imageops::FilterType::Triangle,
    );
    let scale_x = f64::from(MAP_DISPLAY_WIDTH) / f64::from(crop_w);
    let scale_y = f64::from(MAP_DISPLAY_HEIGHT) / f64::from(crop_h);
    let draw_x = (f64::from(local_x) * scale_x).round() as i32;
    let draw_y = (f64::from(local_y) * scale_y).round() as i32;
    draw_pin(&mut output, draw_x, draw_y);
    Ok(Raster {
        width: output.width(),
        height: output.height(),
        pixels: output.into_raw(),
    })
}

fn frame(offset_minutes: u32, now: Timestamp) -> RadarFrame {
    let label = if offset_minutes == 0 {
        "Latest scan".to_string()
    } else {
        format!("{offset_minutes} min ago")
    };
    RadarFrame {
        offset_minutes,
        layer: radar_layer(offset_minutes, now),
        label,
    }
}

fn world(latitude: f64, longitude: f64, zoom: u32) -> (f64, f64) {
    let tiles = f64::from(1u32 << zoom);
    let x = (longitude + 180.0) / 360.0 * tiles;
    let lat_rad = latitude.to_radians();
    let y = (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0 * tiles;
    (x, y)
}

fn clamp_pixel(value: f64) -> u32 {
    round_half_away_from_zero(value).clamp(0, 255) as u32
}

fn blit_tiles(image: &mut RgbaImage, grid: &TileWindow, tiles: &[Option<Vec<u8>>]) {
    for row in 0..grid.span_y {
        for col in 0..grid.span_x {
            let index = (row * grid.span_x + col) as usize;
            let Some(bytes) = tiles.get(index).and_then(Option::as_ref) else {
                continue;
            };
            let Some(tile) = decode_tile(bytes) else {
                continue;
            };
            imageops::overlay(
                image,
                &tile,
                i64::from(col * TILE_SIZE),
                i64::from(row * TILE_SIZE),
            );
        }
    }
}

fn decode_tile(bytes: &[u8]) -> Option<RgbaImage> {
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::Mutex;

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let key = hasher.finish();

    static CACHE: Mutex<Option<HashMap<u64, RgbaImage>>> = Mutex::new(None);
    if let Ok(mut guard) = CACHE.lock() {
        let cache = guard.get_or_insert_with(HashMap::new);
        if let Some(image) = cache.get(&key) {
            return Some(image.clone());
        }
        let image = decode_tile_uncached(bytes)?;
        if cache.len() > 96 {
            cache.clear();
        }
        cache.insert(key, image.clone());
        return Some(image);
    }
    decode_tile_uncached(bytes)
}

fn decode_tile_uncached(bytes: &[u8]) -> Option<RgbaImage> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    if image.width() == TILE_SIZE && image.height() == TILE_SIZE {
        Some(image)
    } else {
        Some(imageops::resize(
            &image,
            TILE_SIZE,
            TILE_SIZE,
            imageops::FilterType::Triangle,
        ))
    }
}

fn draw_pin(image: &mut RgbaImage, x: i32, y: i32) {
    draw_disc(image, x, y - 8, 8, Rgba([255, 255, 255, 255]));
    draw_disc(image, x, y - 8, 5, Rgba([225, 29, 72, 255]));
    draw_disc(image, x, y, 3, Rgba([225, 29, 72, 255]));
}

fn draw_disc(image: &mut RgbaImage, cx: i32, cy: i32, radius: i32, color: Rgba<u8>) {
    let radius_sq = radius * radius;
    let width = image.width() as i32;
    let height = image.height() as i32;
    for py in (cy - radius).max(0)..=(cy + radius).min(height - 1) {
        for px in (cx - radius).max(0)..=(cx + radius).min(width - 1) {
            let dx = px - cx;
            let dy = py - cy;
            if dx * dx + dy * dy <= radius_sq {
                image.put_pixel(px as u32, py as u32, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        APPLET_ZOOM_MAX, APPLET_ZOOM_MIN, GestureCamera, MAP_DISPLAY_HEIGHT, MAP_DISPLAY_WIDTH,
        MapCamera, TileWindow, compose_radar_map, compose_viewport, map_viewport, pin_mercator,
        radar_layer, snapshot_transform, world_to_screen, zoom_toward,
    };
    use image::{ExtendedColorType, ImageEncoder, Rgba, RgbaImage, codecs::png::PngEncoder};
    use jiff::Timestamp;

    #[test]
    fn radar_layer_uses_hsr_latest_and_even_minute_archive() {
        let now: Timestamp = "2026-11-20T05:10:00Z".parse().expect("now");
        assert_eq!(radar_layer(0, now), "q2-hsr-900913");
        assert_eq!(radar_layer(55, now), "mrms::lcref-202611200414");
        assert_eq!(radar_layer(10, now), "mrms::lcref-202611200500");
    }

    #[test]
    fn compose_draws_a_pin_on_a_solid_basemap() {
        let tile = RgbaImage::from_pixel(256, 256, Rgba([10, 20, 200, 255]));
        let mut bytes = Vec::new();
        PngEncoder::new(&mut bytes)
            .write_image(tile.as_raw(), 256, 256, ExtendedColorType::Rgba8)
            .expect("png");
        let grid = TileWindow {
            zoom: 1,
            origin_x: 0,
            origin_y: 0,
            span_x: 1,
            span_y: 1,
            pin_x: 128,
            pin_y: 128,
        };
        let raster = compose_radar_map(&grid, &[Some(bytes)], &[None]).expect("compose");
        assert_eq!(raster.width, MAP_DISPLAY_WIDTH);
        assert_eq!(raster.height, MAP_DISPLAY_HEIGHT);
        let head = pixel(&raster.pixels, raster.width, 200, 122);
        assert!(head[0] > 180, "pin red channel {}", head[0]);
        assert!(head[2] < 120, "pin blue channel {}", head[2]);
        let corner = pixel(&raster.pixels, raster.width, 4, 4);
        assert!(corner[2] > 150, "basemap blue channel {}", corner[2]);
    }

    #[test]
    fn map_viewport_centers_pin_with_zero_pan() {
        let view = map_viewport(MapCamera {
            latitude: 41.6,
            longitude: -93.6,
            zoom: 7,
            pan_x: 0.0,
            pan_y: 0.0,
            view_width: 880,
            view_height: 520,
            margin: 1,
        });
        assert_eq!(view.zoom, 7);
        assert_eq!(view.view_width, 880);
        assert_eq!(view.view_height, 520);
        assert_eq!(view.pin_x, 440);
        assert_eq!(view.pin_y, 260);
        assert!(view.span_x >= 3);
        assert!(view.span_y >= 3);
        assert_eq!(
            view.tile_coords().len(),
            (view.span_x * view.span_y) as usize
        );
    }

    #[test]
    fn map_viewport_pan_moves_pin() {
        let view = map_viewport(MapCamera {
            latitude: 41.6,
            longitude: -93.6,
            zoom: 7,
            pan_x: 40.0,
            pan_y: -20.0,
            view_width: 400,
            view_height: 300,
            margin: 1,
        });
        assert_eq!(view.pin_x, 240);
        assert_eq!(view.pin_y, 130);
    }

    #[test]
    fn map_viewport_clamps_zoom() {
        assert_eq!(
            map_viewport(MapCamera {
                latitude: 0.0,
                longitude: 0.0,
                zoom: 1,
                pan_x: 0.0,
                pan_y: 0.0,
                view_width: 100,
                view_height: 100,
                margin: 0,
            })
            .zoom,
            APPLET_ZOOM_MIN
        );
        assert_eq!(
            map_viewport(MapCamera {
                latitude: 0.0,
                longitude: 0.0,
                zoom: 20,
                pan_x: 0.0,
                pan_y: 0.0,
                view_width: 100,
                view_height: 100,
                margin: 0,
            })
            .zoom,
            APPLET_ZOOM_MAX
        );
    }

    #[test]
    fn compose_viewport_matches_view_size_and_draws_pin() {
        let tile = RgbaImage::from_pixel(256, 256, Rgba([10, 20, 200, 255]));
        let mut bytes = Vec::new();
        PngEncoder::new(&mut bytes)
            .write_image(tile.as_raw(), 256, 256, ExtendedColorType::Rgba8)
            .expect("png");
        let view = map_viewport(MapCamera {
            latitude: 0.0,
            longitude: 0.0,
            zoom: 5,
            pan_x: 0.0,
            pan_y: 0.0,
            view_width: 200,
            view_height: 150,
            margin: 0,
        });
        let tiles: Vec<Option<Vec<u8>>> = (0..(view.span_x * view.span_y))
            .map(|_| Some(bytes.clone()))
            .collect();
        let raster = compose_viewport(&view, &tiles, &vec![None; tiles.len()]).expect("compose");
        assert_eq!(raster.width, 200);
        assert_eq!(raster.height, 150);
        let pin = pixel(
            &raster.pixels,
            raster.width,
            view.pin_x.clamp(0, 199) as u32,
            (view.pin_y - 8).clamp(0, 149) as u32,
        );
        assert!(pin[0] > 180, "pin red channel {}", pin[0]);
    }

    #[test]
    fn snapshot_transform_maps_settled_point_to_live() {
        let latitude = 41.6;
        let longitude = -93.6;
        let width = 400.0;
        let height = 280.0;
        let settled = GestureCamera {
            zoom: 6.25,
            pan_x: 40.0,
            pan_y: -20.0,
        };
        let live = GestureCamera {
            zoom: 7.1,
            pan_x: -15.0,
            pan_y: 30.0,
        };
        let sx = 120.0;
        let sy = 90.0;
        let (pin_x, pin_y) = pin_mercator(latitude, longitude);
        // Invert settled screen → world using continuous mercator.
        let tiles = 2f64.powf(settled.zoom);
        let mx = pin_x + (sx - width / 2.0 - settled.pan_x) / (256.0 * tiles);
        let my = pin_y + (sy - height / 2.0 - settled.pan_y) / (256.0 * tiles);
        let (expected_x, expected_y) =
            world_to_screen(latitude, longitude, live, width, height, mx, my);
        let xform = snapshot_transform(settled, live, width, height);
        let mapped_x = xform.scale * sx + xform.tx;
        let mapped_y = xform.scale * sy + xform.ty;
        assert!((mapped_x - expected_x).abs() < 1e-6);
        assert!((mapped_y - expected_y).abs() < 1e-6);
    }

    #[test]
    fn snapshot_transform_is_identity_when_cameras_match() {
        let camera = GestureCamera {
            zoom: 6.0,
            pan_x: 12.0,
            pan_y: -8.0,
        };
        let xform = snapshot_transform(camera, camera, 320.0, 240.0);
        assert!((xform.scale - 1.0).abs() < 1e-12);
        assert!(xform.tx.abs() < 1e-10);
        assert!(xform.ty.abs() < 1e-10);
    }

    #[test]
    fn zoom_toward_keeps_origin_fixed() {
        let latitude = 35.1;
        let longitude = -106.6;
        let width = 500.0;
        let height = 300.0;
        let origin_x = 200.0;
        let origin_y = 100.0;
        let before = GestureCamera {
            zoom: 6.0,
            pan_x: 0.0,
            pan_y: 0.0,
        };
        let after = zoom_toward(before, 7.0, origin_x, origin_y, width, height);
        let (pin_x, pin_y) = pin_mercator(latitude, longitude);
        let mx = pin_x + (origin_x - width / 2.0 - before.pan_x) / (256.0 * 2f64.powf(before.zoom));
        let my =
            pin_y + (origin_y - height / 2.0 - before.pan_y) / (256.0 * 2f64.powf(before.zoom));
        let (sx, sy) = world_to_screen(latitude, longitude, after, width, height, mx, my);
        assert!((sx - origin_x).abs() < 1e-6);
        assert!((sy - origin_y).abs() < 1e-6);
    }

    fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * width + x) * 4) as usize;
        [
            pixels[index],
            pixels[index + 1],
            pixels[index + 2],
            pixels[index + 3],
        ]
    }
}
