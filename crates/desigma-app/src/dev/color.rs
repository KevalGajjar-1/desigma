//! Copyable CSS color values for the developer inspector.

use desigma_doc::Color;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorFormat {
    #[default]
    Hex,
    Rgb,
    Hsl,
}

impl ColorFormat {
    pub const ALL: [Self; 3] = [Self::Hex, Self::Rgb, Self::Hsl];

    pub fn label(self) -> &'static str {
        match self {
            Self::Hex => "Hex",
            Self::Rgb => "RGB",
            Self::Hsl => "HSL",
        }
    }

    /// `alpha` already includes paint opacity; do not multiply color alpha again.
    pub fn value(self, color: Color, alpha: f32) -> String {
        let alpha = alpha.clamp(0.0, 1.0);
        let [r, g, b] = [color.r, color.g, color.b].map(|v| v.clamp(0.0, 1.0));
        let byte = |v: f32| (v * 255.0).round() as u8;
        let suffix = if alpha < 1.0 { format!(" / {}", decimal(alpha)) } else { String::new() };
        match self {
            Self::Hex => {
                let rgb = format!("#{:02X}{:02X}{:02X}", byte(r), byte(g), byte(b));
                if alpha < 1.0 {
                    format!("{rgb}{:02X}", byte(alpha))
                } else {
                    rgb
                }
            }
            Self::Rgb => format!("rgb({} {} {}{suffix})", byte(r), byte(g), byte(b)),
            Self::Hsl => {
                let hi = r.max(g).max(b);
                let lo = r.min(g).min(b);
                let delta = hi - lo;
                let lightness = (hi + lo) / 2.0;
                let (hue, saturation) = if delta == 0.0 {
                    (0.0, 0.0)
                } else {
                    let hue = if hi == r {
                        ((g - b) / delta).rem_euclid(6.0)
                    } else if hi == g {
                        (b - r) / delta + 2.0
                    } else {
                        (r - g) / delta + 4.0
                    };
                    (hue * 60.0, delta / (1.0 - (2.0 * lightness - 1.0).abs()))
                };
                format!("hsl({} {}% {}%{suffix})", decimal(hue), decimal(saturation * 100.0), decimal(lightness * 100.0))
            }
        }
    }
}

fn decimal(v: f32) -> String {
    format!("{v:.6}").trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_values_include_combined_alpha_once() {
        let c = Color::rgb(255, 0, 0).with_alpha(0.5);
        assert_eq!(ColorFormat::Hex.value(c, 0.25), "#FF000040");
        assert_eq!(ColorFormat::Rgb.value(c, 0.25), "rgb(255 0 0 / 0.25)");
        assert_eq!(ColorFormat::Hsl.value(c, 0.25), "hsl(0 100% 50% / 0.25)");
        assert_eq!(ColorFormat::Rgb.value(c, 0.001), "rgb(255 0 0 / 0.001)");
        assert_eq!(ColorFormat::Hex.value(c, 0.0), "#FF000000");
    }

    #[test]
    fn hsl_handles_achromatic_and_all_hue_sectors() {
        for (c, expected) in [
            (Color::BLACK, "hsl(0 0% 0%)"),
            (Color::WHITE, "hsl(0 0% 100%)"),
            (Color::rgb(255, 255, 0), "hsl(60 100% 50%)"),
            (Color::rgb(0, 255, 0), "hsl(120 100% 50%)"),
            (Color::rgb(0, 255, 255), "hsl(180 100% 50%)"),
            (Color::rgb(0, 0, 255), "hsl(240 100% 50%)"),
            (Color::rgb(255, 0, 255), "hsl(300 100% 50%)"),
        ] {
            assert_eq!(ColorFormat::Hsl.value(c, 1.0), expected);
        }
    }
}
