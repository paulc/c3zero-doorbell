use anyhow::{bail, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RgbLayout {
    Rgb,
    Grb,
}

#[derive(Clone, Copy, Debug)]
pub enum RgbTransform {
    Intensity(f32),
    Rotate,
    Fill(Rgb),
    FillThreshold(Rgb, f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    r: u8,
    g: u8,
    b: u8,
}

pub const OFF: Rgb = Rgb { r: 0, g: 0, b: 0 };
pub const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };
pub const GREEN: Rgb = Rgb { r: 0, g: 255, b: 0 };
pub const BLUE: Rgb = Rgb { r: 0, g: 0, b: 255 };
pub const WHITE: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};

impl Rgb {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }
    /// Converts hue, saturation, value to RGB
    pub fn from_hsv(h: u32, s: u32, v: u32) -> Result<Self> {
        if h > 360 || s > 100 || v > 100 {
            bail!("The given HSV values are not in valid range");
        }
        let s = s as f64 / 100.0;
        let v = v as f64 / 100.0;
        let c = s * v;
        let x = c * (1.0 - (((h as f64 / 60.0) % 2.0) - 1.0).abs());
        let m = v - c;
        let (r, g, b) = match h {
            0..=59 => (c, x, 0.0),
            60..=119 => (x, c, 0.0),
            120..=179 => (0.0, c, x),
            180..=239 => (0.0, x, c),
            240..=299 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        Ok(Self {
            r: ((r + m) * 255.0) as u8,
            g: ((g + m) * 255.0) as u8,
            b: ((b + m) * 255.0) as u8,
        })
    }
    #[inline]
    pub fn from_f32((r, g, b): (f32, f32, f32)) -> Self {
        Self {
            r: (r * 255.0) as u8,
            g: (g * 255.0) as u8,
            b: (b * 255.0) as u8,
        }
    }
    #[inline]
    pub fn to_f32(&self) -> (f32, f32, f32) {
        (
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        )
    }
    #[inline]
    pub fn to_u32(&self, format: RgbLayout) -> u32 {
        match format {
            RgbLayout::Rgb => ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32,
            RgbLayout::Grb => ((self.g as u32) << 16) | ((self.r as u32) << 8) | self.b as u32,
        }
    }
    pub fn transform(&self, transforms: &[RgbTransform]) -> Self {
        let (mut r, mut g, mut b) = self.to_f32();
        for t in transforms {
            (r, g, b) = match t {
                RgbTransform::Fill(rgb) => rgb.to_f32(),
                RgbTransform::FillThreshold(rgb, t) => {
                    if (r + g + b) > *t {
                        (r, g, b)
                    } else {
                        rgb.to_f32()
                    }
                }
                RgbTransform::Rotate => (g, b, r),
                RgbTransform::Intensity(i) => (
                    (r * i).clamp(0.0, 1.0),
                    (g * i).clamp(0.0, 1.0),
                    (b * i).clamp(0.0, 1.0),
                ),
            }
        }
        Self::from_f32((r, g, b))
    }
}

impl Default for Rgb {
    fn default() -> Self {
        OFF
    }
}
