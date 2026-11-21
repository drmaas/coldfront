// SPDX-License-Identifier: GPL-3.0-only

//! Relief (3D) radar: sample n0q echoes into a heightfield and paint an
//! orthographic mesh. The web client keeps a WebGL path with the same rules.

use crate::radar::Raster;

/// One wet radar cell after downsampling. Coordinates are 0–1 across the view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EchoSample {
    pub u: f32,
    pub v: f32,
    /// Relative column height from the n0q color band, 0–1.
    pub height: f32,
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Dense height/color grid for a smooth storm surface.
#[derive(Debug, Clone, PartialEq)]
pub struct EchoField {
    pub cols: usize,
    pub rows: usize,
    pub height: Vec<f32>,
    /// RGB triples in 0–1, length `cols * rows * 3`.
    pub color: Vec<f32>,
}

/// Orbit camera for the relief board.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReliefCamera {
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrthoFit {
    Cover,
    Contain,
}

const COLOR_BANDS: &[(f32, u8, u8, u8, f32, f32)] = &[
    // height, r, g, b, hue_min, hue_max
    (0.1, 0, 220, 235, 165.0, 200.0),
    (0.18, 0, 90, 245, 200.0, 265.0),
    (0.3, 0, 200, 0, 75.0, 165.0),
    (0.58, 245, 230, 0, 45.0, 75.0),
    (0.75, 255, 140, 0, 18.0, 45.0),
    (0.92, 230, 0, 0, 345.0, 378.0),
    (0.92, 230, 0, 0, 0.0, 18.0),
    (0.97, 220, 0, 220, 280.0, 345.0),
    (1.0, 156, 85, 206, 265.0, 280.0),
];

const MIN_ALPHA: u8 = 20;
const STAGE_R: i32 = 213;
const STAGE_G: i32 = 227;
const STAGE_B: i32 = 236;
const MAX_STAGE_DISTANCE: f32 = 28.0;
const MIN_CHROMA: i32 = 28;
pub const DEFAULT_COLS: usize = 200;
pub const DEFAULT_ROWS: usize = 120;

pub const DEFAULT_YAW: f32 = 0.0;
pub const DEFAULT_PITCH: f32 = 1.32;
pub const MIN_DISTANCE: f32 = 1.6;
pub const MAX_DISTANCE: f32 = 7.0;
pub const MIN_PITCH: f32 = 0.55;
pub const MAX_PITCH: f32 = 1.45;
const MAX_HEIGHT: f32 = 0.55;
const GROUND_DEPTH: f32 = 1.85;
const CLEAR: [u8; 4] = [213, 227, 236, 255];
const LIGHT_DIR: [f32; 3] = [-0.48, 0.78, -0.32];
const HEIGHT_CUTOFF: f32 = 0.012;

/// Fine cells so the storm stays a hill, not a stack of blocks.
pub fn echo_grid(width: u32, height: u32) -> (usize, usize) {
    let cell = 2.0_f32;
    let cols = ((width as f32 / cell).round() as usize).clamp(120, 960);
    let rows = ((height as f32 / cell).round() as usize).clamp(80, 640);
    (cols, rows)
}

/// Downsample a radar-only RGBA buffer into echo columns.
pub fn sample_echoes(
    pixels: &[u8],
    width: u32,
    height: u32,
    cols: usize,
    rows: usize,
) -> Vec<EchoSample> {
    if width == 0 || height == 0 || cols == 0 || rows == 0 {
        return Vec::new();
    }
    let width = width as usize;
    let height = height as usize;
    if pixels.len() < width * height * 4 {
        return Vec::new();
    }
    let mut echoes = Vec::new();
    let cell_w = width as f32 / cols as f32;
    let cell_h = height as f32 / rows as f32;

    for row in 0..rows {
        for col in 0..cols {
            let x0 = (col as f32 * cell_w).floor() as usize;
            let y0 = (row as f32 * cell_h).floor() as usize;
            let x1 = ((col + 1) as f32 * cell_w).floor() as usize;
            let y1 = ((row + 1) as f32 * cell_h).floor() as usize;
            let x1 = x1.min(width);
            let y1 = y1.min(height);
            let mut sum_r = 0u32;
            let mut sum_g = 0u32;
            let mut sum_b = 0u32;
            let mut count = 0u32;
            for y in y0..y1 {
                for x in x0..x1 {
                    let i = (y * width + x) * 4;
                    let a = pixels[i + 3];
                    if a < MIN_ALPHA {
                        continue;
                    }
                    let r = pixels[i];
                    let g = pixels[i + 1];
                    let b = pixels[i + 2];
                    if is_pale_stage(r, g, b) || chroma(r, g, b) < MIN_CHROMA {
                        continue;
                    }
                    sum_r += u32::from(r);
                    sum_g += u32::from(g);
                    sum_b += u32::from(b);
                    count += 1;
                }
            }
            if count == 0 {
                continue;
            }
            let r = ((sum_r + count / 2) / count) as u8;
            let g = ((sum_g + count / 2) / count) as u8;
            let b = ((sum_b + count / 2) / count) as u8;
            if chroma(r, g, b) < MIN_CHROMA {
                continue;
            }
            let Some(band) = band_for(r, g, b) else {
                continue;
            };
            echoes.push(EchoSample {
                u: (col as f32 + 0.5) / cols as f32,
                v: (row as f32 + 0.5) / rows as f32,
                height: band.0,
                r: band.1,
                g: band.2,
                b: band.3,
            });
        }
    }
    echoes
}

/// Decode a radar tile PNG, denoise once, and re-encode as PNG.
///
/// Returns `None` when the bytes are not a decodeable image.
#[must_use]
pub fn denoise_radar_png(bytes: &[u8]) -> Option<Vec<u8>> {
    use crate::radar::TILE_SIZE;
    use image::{ExtendedColorType, ImageEncoder, codecs::png::PngEncoder, imageops};

    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let mut image = if image.width() == TILE_SIZE && image.height() == TILE_SIZE {
        image
    } else {
        imageops::resize(&image, TILE_SIZE, TILE_SIZE, imageops::FilterType::Triangle)
    };
    denoise_radar(image.as_mut(), TILE_SIZE, TILE_SIZE);
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(
            image.as_raw(),
            TILE_SIZE,
            TILE_SIZE,
            ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(out)
}

/// Drop isolated n0q speckle in place. Lone returns go transparent.
pub fn denoise_radar(pixels: &mut [u8], width: u32, height: u32) {
    if width == 0 || height == 0 {
        return;
    }
    let width = width as usize;
    let height = height as usize;
    let count = width * height;
    if pixels.len() < count * 4 {
        return;
    }
    let mut echo = vec![0u8; count];
    for (i, mark) in echo.iter_mut().enumerate() {
        let o = i * 4;
        if pixels[o + 3] < MIN_ALPHA {
            continue;
        }
        let r = pixels[o];
        let g = pixels[o + 1];
        let b = pixels[o + 2];
        if is_pale_stage(r, g, b) || chroma(r, g, b) < MIN_CHROMA {
            continue;
        }
        if band_for(r, g, b).is_none() {
            continue;
        }
        *mark = 1;
    }

    let mut core = vec![0u8; count];
    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            if echo[i] == 0 {
                continue;
            }
            let mut neighbors = 0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        continue;
                    }
                    if echo[ny as usize * width + nx as usize] != 0 {
                        neighbors += 1;
                    }
                }
            }
            if neighbors >= 4 {
                core[i] = 1;
            }
        }
    }

    let mut keep = core.clone();
    for y in 0..height {
        for x in 0..width {
            let i = y * width + x;
            if echo[i] == 0 || core[i] != 0 {
                continue;
            }
            let mut touch = false;
            'neighbors: for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= width as i32 || ny >= height as i32 {
                        continue;
                    }
                    if core[ny as usize * width + nx as usize] != 0 {
                        touch = true;
                        break 'neighbors;
                    }
                }
            }
            if touch {
                keep[i] = 1;
            }
        }
    }

    for i in 0..count {
        if echo[i] != 0 && keep[i] == 0 {
            pixels[i * 4 + 3] = 0;
        }
    }
}

/// Rasterize echoes onto the same grid `sample_echoes` used. No extra spread.
pub fn build_echo_field(echoes: &[EchoSample], cols: usize, rows: usize) -> EchoField {
    let mut height = vec![0.0_f32; cols * rows];
    let mut color = vec![0.0_f32; cols * rows * 3];
    for echo in echoes {
        let col = ((echo.u * cols as f32).floor() as isize).clamp(0, cols as isize - 1) as usize;
        let row = ((echo.v * rows as f32).floor() as isize).clamp(0, rows as isize - 1) as usize;
        let i = row * cols + col;
        if echo.height < height[i] {
            continue;
        }
        height[i] = echo.height;
        let o = i * 3;
        color[o] = f32::from(echo.r) / 255.0;
        color[o + 1] = f32::from(echo.g) / 255.0;
        color[o + 2] = f32::from(echo.b) / 255.0;
    }
    let mut smoothed = height;
    for _ in 0..4 {
        smoothed = soften_wet(&smoothed, cols, rows);
    }
    let color = soften_colors(&color, &smoothed, cols, rows);
    EchoField {
        cols,
        rows,
        height: smoothed,
        color,
    }
}

/// Match a view frustum to the canvas aspect.
pub fn fit_ortho_half_extents(half_w: f32, half_h: f32, aspect: f32, mode: OrthoFit) -> (f32, f32) {
    if half_w <= 0.0 || half_h <= 0.0 || aspect <= 0.0 {
        return (half_w, half_h);
    }
    match mode {
        OrthoFit::Cover => {
            if half_w / half_h > aspect {
                (half_h * aspect, half_h)
            } else {
                (half_w, half_w / aspect)
            }
        }
        OrthoFit::Contain => {
            if half_w / half_h > aspect {
                (half_w, half_w / aspect)
            } else {
                (half_h * aspect, half_h)
            }
        }
    }
}

/// Hue in degrees [0, 360).
pub fn hue_degrees(r: u8, g: u8, b: u8) -> f32 {
    let r = f32::from(r);
    let g = f32::from(g);
    let b = f32::from(b);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    if delta < 1.0 {
        return 0.0;
    }
    let mut hue = if (max - r).abs() < f32::EPSILON {
        60.0 * (((g - b) / delta) % 6.0)
    } else if (max - g).abs() < f32::EPSILON {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    if hue < 0.0 {
        hue += 360.0;
    }
    hue
}

/// Band height for a known RGB sample, or `None` when the color is not wet n0q.
pub fn band_height(r: u8, g: u8, b: u8) -> Option<f32> {
    band_for(r, g, b).map(|band| band.0)
}

pub fn default_relief_camera(ground_width: u32, ground_height: u32) -> ReliefCamera {
    let extent = ground_extent(ground_width, ground_height);
    ReliefCamera {
        yaw: DEFAULT_YAW,
        pitch: DEFAULT_PITCH,
        distance: fit_distance(extent.0, extent.1),
    }
}

pub fn relief_camera_locked(camera: &ReliefCamera) -> bool {
    (camera.yaw - DEFAULT_YAW).abs() < 0.001 && (camera.pitch - DEFAULT_PITCH).abs() < 0.001
}

pub fn clamp_relief_pitch(pitch: f32) -> f32 {
    pitch.clamp(MIN_PITCH, MAX_PITCH)
}

pub fn clamp_relief_distance(distance: f32) -> f32 {
    distance.clamp(MIN_DISTANCE, MAX_DISTANCE)
}

/// Paint the relief board: ground texture under an extruded echo heightfield.
pub fn render_relief(ground: &Raster, field: &EchoField, camera: &ReliefCamera) -> Raster {
    let width = ground.width.max(1);
    let height = ground.height.max(1);
    let aspect = width as f32 / height as f32;
    let extent = ground_extent(width, height);
    let locked = relief_camera_locked(camera);
    let view = look_at(camera.yaw, camera.pitch, camera.distance);
    let projection = ortho_for_board(&view, extent, aspect, camera.distance, locked);
    let vp = multiply(&projection, &view);

    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for chunk in pixels.as_chunks_mut::<4>().0 {
        *chunk = CLEAR;
    }
    let mut zbuf = vec![f32::INFINITY; (width * height) as usize];

    draw_ground(&mut pixels, &mut zbuf, width, height, ground, extent, &vp);

    if !locked {
        draw_slab(&mut pixels, &mut zbuf, width, height, extent, &vp);
    }

    draw_field(&mut pixels, &mut zbuf, width, height, field, extent, &vp);

    if !locked {
        draw_mast(&mut pixels, &mut zbuf, width, height, &vp);
    }

    Raster {
        width,
        height,
        pixels,
    }
}

fn ground_extent(width: u32, height: u32) -> (f32, f32) {
    let aspect = if height > 0 {
        width as f32 / height as f32
    } else {
        4.0 / 3.0
    };
    (GROUND_DEPTH * aspect, GROUND_DEPTH)
}

fn fit_distance(width: f32, depth: f32) -> f32 {
    let half_w = width / 2.0;
    let half_d = depth / 2.0;
    (half_w * 2.2).max(half_d * 2.4).clamp(2.5, MAX_DISTANCE)
}

fn chroma(r: u8, g: u8, b: u8) -> i32 {
    i32::from(r.max(g).max(b)) - i32::from(r.min(g).min(b))
}

fn is_pale_stage(r: u8, g: u8, b: u8) -> bool {
    let dr = i32::from(r) - STAGE_R;
    let dg = i32::from(g) - STAGE_G;
    let db = i32::from(b) - STAGE_B;
    ((dr * dr + dg * dg + db * db) as f32).sqrt() <= MAX_STAGE_DISTANCE
}

fn band_for(r: u8, g: u8, b: u8) -> Option<(f32, u8, u8, u8)> {
    let hue = hue_degrees(r, g, b);
    for band in COLOR_BANDS {
        if hue >= band.4 && hue < band.5 {
            return Some((band.0, band.1, band.2, band.3));
        }
    }
    for band in COLOR_BANDS {
        if band.5 > 360.0 && (hue >= band.4 || hue < band.5 - 360.0) {
            return Some((band.0, band.1, band.2, band.3));
        }
    }
    None
}

fn soften_wet(source: &[f32], cols: usize, rows: usize) -> Vec<f32> {
    let mut out = vec![0.0; source.len()];
    for row in 0..rows {
        for col in 0..cols {
            let i = row * cols + col;
            let center = source[i];
            if center <= 0.0 {
                continue;
            }
            let mut sum = center * 4.0;
            let mut weight = 4.0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let x = col as i32 + dx;
                    let y = row as i32 + dy;
                    if x < 0 || y < 0 || x >= cols as i32 || y >= rows as i32 {
                        continue;
                    }
                    let neighbor = source[y as usize * cols + x as usize];
                    if neighbor <= 0.0 {
                        continue;
                    }
                    sum += neighbor;
                    weight += 1.0;
                }
            }
            out[i] = sum / weight;
        }
    }
    out
}

fn soften_colors(source: &[f32], heights: &[f32], cols: usize, rows: usize) -> Vec<f32> {
    let mut out = vec![0.0; source.len()];
    for row in 0..rows {
        for col in 0..cols {
            let i = row * cols + col;
            if heights[i] <= 0.0 {
                continue;
            }
            let o = i * 3;
            let mut r = source[o] * 4.0;
            let mut g = source[o + 1] * 4.0;
            let mut b = source[o + 2] * 4.0;
            let mut weight = 4.0;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let x = col as i32 + dx;
                    let y = row as i32 + dy;
                    if x < 0 || y < 0 || x >= cols as i32 || y >= rows as i32 {
                        continue;
                    }
                    if heights[y as usize * cols + x as usize] <= 0.0 {
                        continue;
                    }
                    let no = (y as usize * cols + x as usize) * 3;
                    r += source[no];
                    g += source[no + 1];
                    b += source[no + 2];
                    weight += 1.0;
                }
            }
            out[o] = r / weight;
            out[o + 1] = g / weight;
            out[o + 2] = b / weight;
        }
    }
    out
}

fn look_at(yaw: f32, pitch: f32, distance: f32) -> [f32; 16] {
    let cx = yaw.sin() * pitch.cos() * distance;
    let cy = pitch.sin() * distance;
    let cz = yaw.cos() * pitch.cos() * distance;
    look_at_matrix(cx, cy, cz, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0)
}

fn ortho_for_board(
    view: &[f32; 16],
    extent: (f32, f32),
    aspect: f32,
    distance: f32,
    locked: bool,
) -> [f32; 16] {
    let hw = extent.0 / 2.0;
    let hd = extent.1 / 2.0;
    let corners: &[[f32; 3]] = if locked {
        &[
            [-hw, 0.0, -hd],
            [hw, 0.0, -hd],
            [-hw, 0.0, hd],
            [hw, 0.0, hd],
        ]
    } else {
        &[
            [-hw, 0.0, -hd],
            [hw, 0.0, -hd],
            [-hw, 0.0, hd],
            [hw, 0.0, hd],
            [-hw, MAX_HEIGHT, -hd],
            [hw, MAX_HEIGHT, hd],
        ]
    };
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for corner in corners {
        let px = view[0] * corner[0] + view[4] * corner[1] + view[8] * corner[2] + view[12];
        let py = view[1] * corner[0] + view[5] * corner[1] + view[9] * corner[2] + view[13];
        min_x = min_x.min(px);
        max_x = max_x.max(px);
        min_y = min_y.min(py);
        max_y = max_y.max(py);
    }
    let dolly = if locked {
        1.0
    } else {
        (distance / 2.9).clamp(0.75, 1.45)
    };
    let pad = if locked { 0.96 } else { 1.02 };
    let mode = if locked {
        OrthoFit::Cover
    } else {
        OrthoFit::Contain
    };
    let (half_w, half_h) = fit_ortho_half_extents(
        ((max_x - min_x) / 2.0) * pad * dolly,
        ((max_y - min_y) / 2.0) * pad * dolly,
        aspect,
        mode,
    );
    let near = 0.05_f32;
    let far = 40.0_f32;
    [
        1.0 / half_w,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0 / half_h,
        0.0,
        0.0,
        0.0,
        0.0,
        -2.0 / (far - near),
        0.0,
        0.0,
        0.0,
        -(far + near) / (far - near),
        1.0,
    ]
}

#[allow(clippy::too_many_arguments)]
fn look_at_matrix(
    eyex: f32,
    eyey: f32,
    eyez: f32,
    centerx: f32,
    centery: f32,
    centerz: f32,
    upx: f32,
    upy: f32,
    upz: f32,
) -> [f32; 16] {
    let mut zx = eyex - centerx;
    let mut zy = eyey - centery;
    let mut zz = eyez - centerz;
    let mut len = 1.0 / (zx * zx + zy * zy + zz * zz).sqrt();
    zx *= len;
    zy *= len;
    zz *= len;
    let mut xx = upy * zz - upz * zy;
    let mut xy = upz * zx - upx * zz;
    let mut xz = upx * zy - upy * zx;
    len = (xx * xx + xy * xy + xz * xz).sqrt();
    if len < 1e-6 {
        xx = 0.0;
        xy = 0.0;
        xz = 0.0;
    } else {
        len = 1.0 / len;
        xx *= len;
        xy *= len;
        xz *= len;
    }
    let yx = zy * xz - zz * xy;
    let yy = zz * xx - zx * xz;
    let yz = zx * xy - zy * xx;
    [
        xx,
        yx,
        zx,
        0.0,
        xy,
        yy,
        zy,
        0.0,
        xz,
        yz,
        zz,
        0.0,
        -(xx * eyex + xy * eyey + xz * eyez),
        -(yx * eyex + yy * eyey + yz * eyez),
        -(zx * eyex + zy * eyey + zz * eyez),
        1.0,
    ]
}

fn multiply(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0_f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] = a[row] * b[col * 4]
                + a[4 + row] * b[col * 4 + 1]
                + a[8 + row] * b[col * 4 + 2]
                + a[12 + row] * b[col * 4 + 3];
        }
    }
    out
}

fn transform_point(m: &[f32; 16], x: f32, y: f32, z: f32) -> (f32, f32, f32) {
    (
        m[0] * x + m[4] * y + m[8] * z + m[12],
        m[1] * x + m[5] * y + m[9] * z + m[13],
        m[2] * x + m[6] * y + m[10] * z + m[14],
    )
}

fn to_screen(px: f32, py: f32, width: u32, height: u32) -> (f32, f32) {
    let sx = (px * 0.5 + 0.5) * width as f32;
    let sy = (1.0 - (py * 0.5 + 0.5)) * height as f32;
    (sx, sy)
}

#[derive(Clone, Copy)]
struct Vert {
    x: f32,
    y: f32,
    z: f32,
    r: f32,
    g: f32,
    b: f32,
    u: f32,
    v: f32,
    textured: bool,
}

fn draw_ground(
    pixels: &mut [u8],
    zbuf: &mut [f32],
    width: u32,
    height: u32,
    ground: &Raster,
    extent: (f32, f32),
    vp: &[f32; 16],
) {
    let corners = [(0.0_f32, 0.0_f32), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
    let mut verts = [Vert {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        r: 1.0,
        g: 1.0,
        b: 1.0,
        u: 0.0,
        v: 0.0,
        textured: true,
    }; 4];
    for (i, (u, v)) in corners.iter().enumerate() {
        let wx = (u - 0.5) * extent.0;
        let wz = (v - 0.5) * extent.1;
        let (px, py, pz) = transform_point(vp, wx, 0.0, wz);
        let (sx, sy) = to_screen(px, py, width, height);
        verts[i] = Vert {
            x: sx,
            y: sy,
            z: pz,
            r: 1.0,
            g: 1.0,
            b: 1.0,
            u: *u,
            v: *v,
            textured: true,
        };
    }
    raster_tri(
        pixels,
        zbuf,
        width,
        height,
        verts[0],
        verts[1],
        verts[2],
        Some(ground),
    );
    raster_tri(
        pixels,
        zbuf,
        width,
        height,
        verts[0],
        verts[2],
        verts[3],
        Some(ground),
    );
}

fn draw_slab(
    pixels: &mut [u8],
    zbuf: &mut [f32],
    width: u32,
    height: u32,
    extent: (f32, f32),
    vp: &[f32; 16],
) {
    let thick = 0.045_f32;
    let faces: &[([[f32; 3]; 4], [f32; 3])] = &[
        (
            [
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            [0.0, 0.0, -1.0],
        ),
        (
            [
                [0.0, 0.0, 1.0],
                [0.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
                [1.0, 0.0, 1.0],
            ],
            [0.0, 0.0, 1.0],
        ),
        (
            [
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 1.0, 1.0],
                [0.0, 0.0, 1.0],
            ],
            [-1.0, 0.0, 0.0],
        ),
        (
            [
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                [1.0, 1.0, 0.0],
            ],
            [1.0, 0.0, 0.0],
        ),
        (
            [
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [1.0, 0.0, 0.0],
            ],
            [0.0, -1.0, 0.0],
        ),
    ];
    let light = normalize3(LIGHT_DIR);
    for (face, normal) in faces {
        let mut verts = [Vert {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            r: 0.0,
            g: 0.0,
            b: 0.0,
            u: 0.0,
            v: 0.0,
            textured: false,
        }; 4];
        for (i, p) in face.iter().enumerate() {
            let wx = (p[0] - 0.5) * extent.0;
            let wy = p[1] * thick - thick;
            let wz = (p[2] - 0.5) * extent.1;
            let color = mix3([0.42, 0.48, 0.52], [0.28, 0.32, 0.36], p[1]);
            let ndotl = (dot3(*normal, light)).max(0.0);
            let lit = [
                color[0] * (0.35 + 0.55 * ndotl),
                color[1] * (0.35 + 0.55 * ndotl),
                color[2] * (0.35 + 0.55 * ndotl),
            ];
            let (px, py, pz) = transform_point(vp, wx, wy, wz);
            let (sx, sy) = to_screen(px, py, width, height);
            verts[i] = Vert {
                x: sx,
                y: sy,
                z: pz,
                r: lit[0],
                g: lit[1],
                b: lit[2],
                u: 0.0,
                v: 0.0,
                textured: false,
            };
        }
        raster_tri(
            pixels, zbuf, width, height, verts[0], verts[1], verts[2], None,
        );
        raster_tri(
            pixels, zbuf, width, height, verts[0], verts[2], verts[3], None,
        );
    }
}

fn draw_mast(pixels: &mut [u8], zbuf: &mut [f32], width: u32, height: u32, vp: &[f32; 16]) {
    let mast_h = 0.18_f32;
    let faces: &[[[f32; 3]; 4]] = &[
        [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        [
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 1.0],
        ],
        [
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [0.0, 0.0, 1.0],
        ],
        [
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
        ],
        [
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ],
        [
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0],
        ],
    ];
    for face in faces {
        let mut verts = [Vert {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            r: 0.0,
            g: 0.0,
            b: 0.0,
            u: 0.0,
            v: 0.0,
            textured: false,
        }; 4];
        for (i, p) in face.iter().enumerate() {
            let wx = (p[0] - 0.5) * 0.016;
            let wy = p[1] * mast_h + 0.001;
            let wz = (p[2] - 0.5) * 0.016;
            let color = if p[1] > 0.85 {
                [1.0, 1.0, 1.0]
            } else {
                [0.88, 0.12, 0.3]
            };
            let (px, py, pz) = transform_point(vp, wx, wy, wz);
            let (sx, sy) = to_screen(px, py, width, height);
            verts[i] = Vert {
                x: sx,
                y: sy,
                z: pz,
                r: color[0],
                g: color[1],
                b: color[2],
                u: 0.0,
                v: 0.0,
                textured: false,
            };
        }
        raster_tri(
            pixels, zbuf, width, height, verts[0], verts[1], verts[2], None,
        );
        raster_tri(
            pixels, zbuf, width, height, verts[0], verts[2], verts[3], None,
        );
    }
}

fn draw_field(
    pixels: &mut [u8],
    zbuf: &mut [f32],
    width: u32,
    height: u32,
    field: &EchoField,
    extent: (f32, f32),
    vp: &[f32; 16],
) {
    let cols = field.cols;
    let rows = field.rows;
    if cols == 0 || rows == 0 {
        return;
    }
    let height_at = |c: usize, r: usize| field.height[r * cols + c];
    let color_at = |c: usize, r: usize| {
        let o = (r * cols + c) * 3;
        [field.color[o], field.color[o + 1], field.color[o + 2]]
    };

    let vcols = cols + 1;
    let vrows = rows + 1;
    let mut vertex_height = vec![0.0_f32; vcols * vrows];
    let mut vertex_weight = vec![0.0_f32; vcols * vrows];
    let vindex = |c: usize, r: usize| r * vcols + c;
    for r in 0..rows {
        for c in 0..cols {
            let h = height_at(c, r);
            if h <= HEIGHT_CUTOFF {
                continue;
            }
            let y = h * MAX_HEIGHT;
            for dr in 0..=1 {
                for dc in 0..=1 {
                    let i = vindex(c + dc, r + dr);
                    vertex_height[i] += y;
                    vertex_weight[i] += 1.0;
                }
            }
        }
    }
    for i in 0..vertex_height.len() {
        if vertex_weight[i] > 0.0 {
            vertex_height[i] /= vertex_weight[i];
        }
    }
    let sample_height = |c: isize, r: isize, fallback: f32| -> f32 {
        if c < 0 || r < 0 || c >= vcols as isize || r >= vrows as isize {
            return fallback;
        }
        let i = vindex(c as usize, r as usize);
        if vertex_weight[i] <= 0.0 {
            fallback
        } else {
            vertex_height[i]
        }
    };
    let dx = extent.0 / cols as f32;
    let dz = extent.1 / rows as f32;
    let normal_at = |c: usize, r: usize| -> [f32; 3] {
        let y = sample_height(c as isize, r as isize, 0.0);
        let nx = (sample_height(c as isize - 1, r as isize, y)
            - sample_height(c as isize + 1, r as isize, y))
            / (2.0 * dx);
        let nz = (sample_height(c as isize, r as isize - 1, y)
            - sample_height(c as isize, r as isize + 1, y))
            / (2.0 * dz);
        normalize3([nx, 1.0, nz])
    };
    let light = normalize3(LIGHT_DIR);

    let corner = |c: usize, r: usize| {
        let u = c as f32 / cols as f32;
        let v = r as f32 / rows as f32;
        let y = sample_height(c as isize, r as isize, 0.0);
        let n = normal_at(c, r);
        ((u - 0.5) * extent.0, y, (v - 0.5) * extent.1, n, u, v)
    };

    for r in 0..rows {
        for c in 0..cols {
            if height_at(c, r) <= HEIGHT_CUTOFF {
                continue;
            }
            let col = color_at(c, r);
            let corners = [
                corner(c, r),
                corner(c + 1, r),
                corner(c + 1, r + 1),
                corner(c, r + 1),
            ];
            let mut verts = [Vert {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                r: 0.0,
                g: 0.0,
                b: 0.0,
                u: 0.0,
                v: 0.0,
                textured: false,
            }; 4];
            for (i, (wx, wy, wz, n, _u, _v)) in corners.iter().enumerate() {
                if *wy <= 0.004 {
                    // Match shader discard for flat cells.
                }
                let peak = (*wy / MAX_HEIGHT).clamp(0.0, 1.0);
                let shade = 0.72 + (1.22 - 0.72) * peak;
                let base = [col[0] * shade, col[1] * shade, col[2] * shade];
                let mut ndotl = (dot3(*n, light)).max(0.0);
                ndotl = ndotl * 0.35 + 0.65;
                let lit = [
                    base[0] * (0.62 + 0.4 * ndotl),
                    base[1] * (0.62 + 0.4 * ndotl),
                    base[2] * (0.62 + 0.4 * ndotl),
                ];
                let (px, py, pz) = transform_point(vp, *wx, *wy, *wz);
                let (sx, sy) = to_screen(px, py, width, height);
                verts[i] = Vert {
                    x: sx,
                    y: sy,
                    z: pz,
                    r: lit[0],
                    g: lit[1],
                    b: lit[2],
                    u: 0.0,
                    v: 0.0,
                    textured: false,
                };
            }
            // Skip nearly flat discarded triangles (all verts below cutoff).
            if corners.iter().all(|c| c.1 <= 0.004) {
                continue;
            }
            raster_tri(
                pixels, zbuf, width, height, verts[0], verts[1], verts[2], None,
            );
            raster_tri(
                pixels, zbuf, width, height, verts[0], verts[2], verts[3], None,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn raster_tri(
    pixels: &mut [u8],
    zbuf: &mut [f32],
    width: u32,
    height: u32,
    a: Vert,
    b: Vert,
    c: Vert,
    ground: Option<&Raster>,
) {
    let min_x = a.x.min(b.x).min(c.x).floor().max(0.0) as i32;
    let max_x = a.x.max(b.x).max(c.x).ceil().min(width as f32 - 1.0) as i32;
    let min_y = a.y.min(b.y).min(c.y).floor().max(0.0) as i32;
    let max_y = a.y.max(b.y).max(c.y).ceil().min(height as f32 - 1.0) as i32;
    if min_x > max_x || min_y > max_y {
        return;
    }
    let area = edge(a.x, a.y, b.x, b.y, c.x, c.y);
    if area.abs() < 1e-6 {
        return;
    }
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let w0 = edge(b.x, b.y, c.x, c.y, px, py) / area;
            let w1 = edge(c.x, c.y, a.x, a.y, px, py) / area;
            let w2 = edge(a.x, a.y, b.x, b.y, px, py) / area;
            if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                continue;
            }
            let z = w0 * a.z + w1 * b.z + w2 * c.z;
            let idx = (y as u32 * width + x as u32) as usize;
            if z >= zbuf[idx] {
                continue;
            }
            let (r, g, b) = if a.textured {
                let Some(ground) = ground else {
                    continue;
                };
                let u = w0 * a.u + w1 * b.u + w2 * c.u;
                let v = w0 * a.v + w1 * b.v + w2 * c.v;
                sample_ground(ground, u, v)
            } else {
                (
                    (w0 * a.r + w1 * b.r + w2 * c.r).clamp(0.0, 1.0),
                    (w0 * a.g + w1 * b.g + w2 * c.g).clamp(0.0, 1.0),
                    (w0 * a.b + w1 * b.b + w2 * c.b).clamp(0.0, 1.0),
                )
            };
            zbuf[idx] = z;
            let o = idx * 4;
            pixels[o] = (r * 255.0).round() as u8;
            pixels[o + 1] = (g * 255.0).round() as u8;
            pixels[o + 2] = (b * 255.0).round() as u8;
            pixels[o + 3] = 255;
        }
    }
}

fn edge(ax: f32, ay: f32, bx: f32, by: f32, cx: f32, cy: f32) -> f32 {
    (cx - ax) * (by - ay) - (cy - ay) * (bx - ax)
}

fn sample_ground(ground: &Raster, u: f32, v: f32) -> (f32, f32, f32) {
    let w = ground.width.max(1);
    let h = ground.height.max(1);
    let x = (u.clamp(0.0, 1.0) * (w - 1) as f32).round() as u32;
    let y = (v.clamp(0.0, 1.0) * (h - 1) as f32).round() as u32;
    let i = ((y * w + x) * 4) as usize;
    if i + 2 >= ground.pixels.len() {
        return (
            CLEAR[0] as f32 / 255.0,
            CLEAR[1] as f32 / 255.0,
            CLEAR[2] as f32 / 255.0,
        );
    }
    (
        f32::from(ground.pixels[i]) / 255.0,
        f32::from(ground.pixels[i + 1]) / 255.0,
        f32::from(ground.pixels[i + 2]) / 255.0,
    )
}

fn normalize3(v: [f32; 3]) -> [f32; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len < 1e-6 {
        return [0.0, 1.0, 0.0];
    }
    [v[0] / len, v[1] / len, v[2] / len]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(width: u32, height: u32, fill: impl Fn(u32, u32, &mut [u8])) -> (Vec<u8>, u32, u32) {
        let mut data = vec![0u8; (width * height * 4) as usize];
        for y in 0..height {
            for x in 0..width {
                let i = ((y * width + x) * 4) as usize;
                fill(x, y, &mut data[i..i + 4]);
            }
        }
        (data, width, height)
    }

    #[test]
    fn sample_echoes_drops_transparent_cells() {
        let (data, w, h) = rgba(4, 4, |_x, _y, px| {
            px[0] = 0;
            px[1] = 255;
            px[2] = 0;
            px[3] = 0;
        });
        assert!(sample_echoes(&data, w, h, 2, 2).is_empty());
    }

    #[test]
    fn sample_echoes_drops_pale_stage() {
        let (data, w, h) = rgba(4, 4, |_x, _y, px| {
            px[0] = 213;
            px[1] = 227;
            px[2] = 236;
            px[3] = 255;
        });
        assert!(sample_echoes(&data, w, h, 2, 2).is_empty());
    }

    #[test]
    fn green_shorter_than_yellow_shorter_than_red() {
        let (data, w, h) = rgba(12, 4, |x, _y, px| {
            if x < 4 {
                px[0] = 0;
                px[1] = 200;
                px[2] = 0;
            } else if x < 8 {
                px[0] = 245;
                px[1] = 230;
                px[2] = 0;
            } else {
                px[0] = 230;
                px[1] = 0;
                px[2] = 0;
            }
            px[3] = 255;
        });
        let echoes = sample_echoes(&data, w, h, 3, 1);
        assert_eq!(echoes.len(), 3);
        let green = echoes.iter().find(|e| e.u < 1.0 / 3.0).unwrap();
        let yellow = echoes
            .iter()
            .find(|e| e.u > 1.0 / 3.0 && e.u < 2.0 / 3.0)
            .unwrap();
        let red = echoes.iter().find(|e| e.u > 2.0 / 3.0).unwrap();
        assert!(green.height < yellow.height);
        assert!(yellow.height < red.height);
        assert!(green.g > green.r);
        assert!(yellow.r > 0 && yellow.g > 0);
        assert!(red.r > red.g);
    }

    #[test]
    fn keeps_grid_coordinates_inside_unit_square() {
        let (data, w, h) = rgba(6, 6, |_x, _y, px| {
            px[0] = 255;
            px[1] = 0;
            px[2] = 0;
            px[3] = 255;
        });
        for echo in sample_echoes(&data, w, h, 3, 3) {
            assert!(echo.u > 0.0 && echo.u < 1.0);
            assert!(echo.v > 0.0 && echo.v < 1.0);
        }
    }

    #[test]
    fn keeps_anti_aliased_iem_greens() {
        let (data, w, h) = rgba(4, 4, |_x, _y, px| {
            px[0] = 53;
            px[1] = 214;
            px[2] = 91;
            px[3] = 255;
        });
        let echoes = sample_echoes(&data, w, h, 2, 2);
        assert!(!echoes.is_empty());
        assert!(echoes[0].height > 0.0);
    }

    #[test]
    fn denoise_drops_speckle_keeps_cluster() {
        let (mut data, w, h) = rgba(6, 6, |x, y, px| {
            let speck = x == 0 && y == 0;
            let block = x >= 3 && y >= 3;
            if !speck && !block {
                return;
            }
            px[0] = 0;
            px[1] = 200;
            px[2] = 0;
            px[3] = 255;
        });
        denoise_radar(&mut data, w, h);
        assert_eq!(data[3], 0);
        assert_eq!(data[((3 * 6 + 3) * 4 + 3) as usize], 255);
        assert!(!sample_echoes(&data, w, h, 3, 3).is_empty());
    }

    #[test]
    fn drops_near_gray_noise() {
        let (data, w, h) = rgba(4, 4, |_x, _y, px| {
            px[0] = 154;
            px[1] = 162;
            px[2] = 180;
            px[3] = 255;
        });
        assert!(sample_echoes(&data, w, h, 2, 2).is_empty());
    }

    #[test]
    fn builds_height_field_from_echoes() {
        let (data, w, h) = rgba(8, 4, |x, _y, px| {
            if x < 4 {
                px[0] = 0;
                px[1] = 200;
                px[2] = 0;
            } else {
                px[0] = 230;
                px[1] = 0;
                px[2] = 0;
            }
            px[3] = 255;
        });
        let echoes = sample_echoes(&data, w, h, 4, 2);
        let field = build_echo_field(&echoes, 4, 2);
        assert_eq!(field.height.len(), 8);
        assert!(field.height.iter().cloned().fold(0.0_f32, f32::max) > 0.2);
    }

    #[test]
    fn fit_ortho_cover_and_contain() {
        let (half_w, half_h) = fit_ortho_half_extents(2.0, 1.0, 4.0 / 3.0, OrthoFit::Cover);
        assert!((half_w / half_h - 4.0 / 3.0).abs() < 1e-5);
        assert!(half_w < 2.0);
        assert!((half_h - 1.0).abs() < 1e-5);

        let (half_w, half_h) = fit_ortho_half_extents(1.0, 2.0, 4.0 / 3.0, OrthoFit::Contain);
        assert!((half_w / half_h - 4.0 / 3.0).abs() < 1e-5);
        assert!(half_w > 1.0);
        assert!((half_h - 2.0).abs() < 1e-5);
    }

    #[test]
    fn render_relief_keeps_ground_when_empty() {
        let ground = Raster {
            width: 8,
            height: 6,
            pixels: [10, 20, 30, 255].repeat(8 * 6),
        };
        let field = build_echo_field(&[], 4, 3);
        let camera = default_relief_camera(8, 6);
        let out = render_relief(&ground, &field, &camera);
        assert_eq!(out.width, 8);
        assert_eq!(out.height, 6);
        assert_eq!(out.pixels.len(), 8 * 6 * 4);
    }
}
