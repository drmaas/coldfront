// SPDX-License-Identifier: GPL-3.0-only

//! Hourly temperature chart and daily range-bar placement.
//!
//! Matches `chartPoints` and `rangePlacement` in `apps/coldfront-web/lib/domain.ts`.

/// One point on the hourly temperature polyline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChartPoint {
    pub x: f64,
    pub y: f64,
}

/// Left edge and width of a daily high/low bar, as percentages of the track.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangePlacement {
    pub left: f64,
    pub width: f64,
}

/// Map display temperatures into chart coordinates for a fixed column width.
#[must_use]
pub fn chart_points(values: &[f64], column_width: f64, height: f64, pad: f64) -> Vec<ChartPoint> {
    if values.is_empty() {
        return Vec::new();
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = if (max - min).abs() < f64::EPSILON {
        1.0
    } else {
        max - min
    };
    values
        .iter()
        .enumerate()
        .map(|(index, value)| ChartPoint {
            x: column_width * index as f64 + column_width / 2.0,
            y: pad + (1.0 - (value - min) / span) * (height - pad * 2.0),
        })
        .collect()
}

/// Place a day's low/high bar on a shared 10-day temperature scale.
#[must_use]
pub fn range_placement(low: f64, high: f64, scale_min: f64, scale_max: f64) -> RangePlacement {
    let span = if (scale_max - scale_min).abs() < f64::EPSILON {
        1.0
    } else {
        scale_max - scale_min
    };
    let left = ((low - scale_min) / span * 100.0).clamp(0.0, 100.0);
    let right = ((high - scale_min) / span * 100.0).clamp(0.0, 100.0);
    let min_width = if (high - low).abs() < f64::EPSILON {
        2.0
    } else {
        0.0
    };
    RangePlacement {
        left,
        width: (right - left).max(min_width),
    }
}

#[cfg(test)]
mod tests {
    use super::{chart_points, range_placement};

    #[test]
    fn chart_points_match_web_sample() {
        let points = chart_points(&[51.0, 50.0, 48.0, 47.0, 46.0], 64.0, 64.0, 8.0);
        assert_eq!(points.len(), 5);
        assert!((points[0].x - 32.0).abs() < 0.001);
        assert!((points[0].y - 8.0).abs() < 0.001);
        assert!((points[4].x - 288.0).abs() < 0.001);
        assert!((points[4].y - 56.0).abs() < 0.001);
    }

    #[test]
    fn range_placement_matches_web_sample() {
        let bar = range_placement(47.0, 51.0, 46.0, 51.0);
        assert!((bar.left - 20.0).abs() < 0.001);
        assert!((bar.width - 80.0).abs() < 0.001);
        let equal = range_placement(50.0, 50.0, 46.0, 51.0);
        assert!((equal.left - 80.0).abs() < 0.001);
        assert!((equal.width - 2.0).abs() < 0.001);
    }
}
