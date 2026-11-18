// SPDX-License-Identifier: GPL-3.0-only

//! Iowa State Mesonet NEXRAD tile URLs and a small raster compositor.
//!
//! libcosmic has no map widget. The applet fetches these tiles and draws the
//! composited image. The web app uses the same URL and frame list.

use crate::error::Error;
use crate::units::round_half_away_from_zero;
use image::{Rgba, RgbaImage, imageops};

pub const IEM_TILE_ORIGIN: &str = "https://mesonet.agron.iastate.edu/cache/tile.py/1.0.0";
pub const MAP_DISPLAY_WIDTH: u32 = 400;
pub const MAP_DISPLAY_HEIGHT: u32 = 260;
const TILE_SIZE: u32 = 256;

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

/// Twelve frames, every five minutes from 55 minutes ago through the latest scan.
#[must_use]
pub fn web_radar_frames() -> Vec<RadarFrame> {
    (0..=11).rev().map(|step| frame(step * 5)).collect()
}

/// Six frames, every ten minutes. Fewer tiles than the web loop, so the popup
/// can show the latest scan quickly and then animate.
#[must_use]
pub fn applet_radar_frames() -> Vec<RadarFrame> {
    [50, 40, 30, 20, 10, 0].into_iter().map(frame).collect()
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

fn frame(offset_minutes: u32) -> RadarFrame {
    let layer = if offset_minutes == 0 {
        "nexrad-n0q-900913".to_string()
    } else {
        format!("nexrad-n0q-900913-m{offset_minutes:02}m")
    };
    let label = if offset_minutes == 0 {
        "Latest scan".to_string()
    } else {
        format!("{offset_minutes} min ago")
    };
    RadarFrame {
        offset_minutes,
        layer,
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
    use super::{MAP_DISPLAY_HEIGHT, MAP_DISPLAY_WIDTH, TileWindow, compose_radar_map};
    use image::{ExtendedColorType, ImageEncoder, Rgba, RgbaImage, codecs::png::PngEncoder};

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
