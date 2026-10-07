// SPDX-License-Identifier: MIT
// Selected tint algorithm and palette adapted from umya-spreadsheet.
// Copyright (c) 2020 MathNya. See third_party/ports.json.
use crate::{ArgbLiteral, Color, ColorKind, Result, ThemeCatalog};

const INDEXED: [u32; 64] = [
    0xFF000000, 0xFFFFFFFF, 0xFFFF0000, 0xFF00FF00, 0xFF0000FF, 0xFFFFFF00, 0xFFFF00FF, 0xFF00FFFF,
    0xFF000000, 0xFFFFFFFF, 0xFFFF0000, 0xFF00FF00, 0xFF0000FF, 0xFFFFFF00, 0xFFFF00FF, 0xFF00FFFF,
    0xFF800000, 0xFF008000, 0xFF000080, 0xFF808000, 0xFF800080, 0xFF008080, 0xFFC0C0C0, 0xFF808080,
    0xFF9999FF, 0xFF993366, 0xFFFFFFCC, 0xFFCCFFFF, 0xFF660066, 0xFFFF8080, 0xFF0066CC, 0xFFCCCCFF,
    0xFF000080, 0xFFFF00FF, 0xFFFFFF00, 0xFF00FFFF, 0xFF800080, 0xFF800000, 0xFF008080, 0xFF0000FF,
    0xFF00CCFF, 0xFFCCFFFF, 0xFFCCFFCC, 0xFFFFFF99, 0xFF99CCFF, 0xFFFF99CC, 0xFFCC99FF, 0xFFFFCC99,
    0xFF3366FF, 0xFF33CCCC, 0xFF99CC00, 0xFFFFCC00, 0xFFFF9900, 0xFFFF6600, 0xFF666699, 0xFF969696,
    0xFF003366, 0xFF339966, 0xFF003300, 0xFF333300, 0xFF993300, 0xFF993366, 0xFF333399, 0xFF333333,
];

// Retain the selected source's 255-step HLS quantization without strings or allocation.
fn tinted(channels: u32, tint: f64) -> u32 {
    if tint == 0.0 {
        return channels;
    }
    let r = f64::from((channels >> 16) & 255) / 255.0;
    let g = f64::from((channels >> 8) & 255) / 255.0;
    let b = f64::from(channels & 255) / 255.0;
    let minimum = r.min(g).min(b);
    let maximum = r.max(g).max(b);
    let delta = maximum - minimum;
    let mut lightness = (minimum + maximum) / 2.0;
    let (mut hue, mut saturation) = if delta == 0.0 {
        (0.0, 0.0)
    } else {
        let saturation = if lightness <= 0.5 {
            delta / (maximum + minimum)
        } else {
            delta / (2.0 - maximum - minimum)
        };
        let rc = (maximum - r) / delta;
        let gc = (maximum - g) / delta;
        let bc = (maximum - b) / delta;
        let hue = if r == maximum {
            bc - gc
        } else if g == maximum {
            2.0 + rc - bc
        } else {
            4.0 + gc - rc
        };
        ((hue / 6.0).rem_euclid(1.0), saturation)
    };
    hue = (hue * 255.0).round() / 255.0;
    saturation = (saturation * 255.0).round() / 255.0;
    lightness = (lightness * 255.0).round();
    lightness = if tint < 0.0 {
        lightness * (1.0 + tint)
    } else {
        lightness * (1.0 - tint) + 255.0 * tint
    };
    lightness = lightness.round() / 255.0;
    let rgb = if saturation == 0.0 {
        let channel = (lightness * 255.0).round() as u32;
        (channel << 16) | (channel << 8) | channel
    } else {
        let first = if lightness < 0.5 {
            lightness * (1.0 + saturation)
        } else {
            lightness + saturation - lightness * saturation
        };
        let second = 2.0 * lightness - first;
        let channel = |h: f64| {
            let h = h.rem_euclid(1.0);
            let value = if 6.0 * h < 1.0 {
                second + (first - second) * 6.0 * h
            } else if 2.0 * h < 1.0 {
                first
            } else if 3.0 * h < 2.0 {
                second + (first - second) * (2.0 / 3.0 - h) * 6.0
            } else {
                second
            };
            (value * 255.0).round().clamp(0.0, 255.0) as u32
        };
        (channel(hue + 1.0 / 3.0) << 16) | (channel(hue) << 8) | channel(hue - 1.0 / 3.0)
    };
    (channels & 0xFF000000) | rgb
}
impl ThemeCatalog {
    /// Resolve portable RGB without changing the original color identity or tint.
    /// Explicit ARGB retains alpha; theme/default palette colors are opaque.
    /// Automatic/system colors without a fallback and unknown slots return None.
    /// Uses the pinned source's 255-step HLS tint quantization.
    pub fn resolve_color(&self, color: &Color, indexed: &[ArgbLiteral]) -> Result<Option<u32>> {
        color.validate()?;
        let channels = match &color.kind {
            ColorKind::Argb(value) => Some(*value),
            ColorKind::ArgbLiteral(value) => Some(value.channels()),
            ColorKind::Theme(index) => index
                .as_i64()
                .and_then(|v| usize::try_from(v).ok())
                .and_then(|index| self.colors.get(index))
                .and_then(Option::as_ref)
                .and_then(crate::ThemeColor::rgb)
                .map(|v| v | 0xFF000000),
            ColorKind::Indexed(index) => index
                .as_i64()
                .and_then(|v| usize::try_from(v).ok())
                .and_then(|index| {
                    if indexed.is_empty() {
                        INDEXED.get(index).copied()
                    } else {
                        indexed.get(index).map(|v| v.channels())
                    }
                }),
            ColorKind::Auto(_) | ColorKind::Unspecified => None,
        };
        Ok(channels.map(|value| tinted(value, color.tint.unwrap_or(0.0))))
    }
}
