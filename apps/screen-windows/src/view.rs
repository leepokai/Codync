//! What the phone shows of a display: the crop it zoomed to and the size to encode (see the iOS `ScreenViewport`).

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Largest frame the encoder gets, whatever the display.
const MAX_LONG: f64 = 3840.0;
const MAX_SHORT: f64 = 2160.0;

/// The crop and encoded size for one `view` message on a display of `width`×`height` pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub crop: Rect,
    pub size: (u32, u32),
    /// The phone asked for a specific crop (else the whole display).
    pub zoomed: bool,
}

impl View {
    /// The whole display, fitted to the largest frame size.
    pub fn whole(width: u32, height: u32) -> Self {
        let crop = Rect {
            x: 0.0,
            y: 0.0,
            w: f64::from(width),
            h: f64::from(height),
        };
        Self {
            crop,
            size: fit(crop, MAX_LONG, MAX_SHORT),
            zoomed: false,
        }
    }

    /// Applies a `view` message: `crop` as `[x, y, w, h]` in pixels, `long`/`short` the phone's viewport.
    pub fn apply(msg: &Value, width: u32, height: u32) -> Self {
        let (dw, dh) = (f64::from(width), f64::from(height));
        let crop = msg["crop"].as_array().and_then(|c| {
            let v: Vec<f64> = c.iter().filter_map(Value::as_f64).collect();
            (v.len() == 4 && v[2] >= 16.0 && v[3] >= 16.0).then(|| {
                let x = v[0].clamp(0.0, dw - 16.0);
                let y = v[1].clamp(0.0, dh - 16.0);
                Rect {
                    x,
                    y,
                    w: v[2].min(dw - x),
                    h: v[3].min(dh - y),
                }
            })
        });
        let whole = Rect {
            x: 0.0,
            y: 0.0,
            w: dw,
            h: dh,
        };
        let rect = crop.unwrap_or(whole);
        let (long, short) = match (msg["long"].as_f64(), msg["short"].as_f64()) {
            (Some(long), Some(short)) => (long.min(MAX_LONG), short.min(MAX_SHORT)),
            _ => (MAX_LONG, MAX_SHORT),
        };
        Self {
            crop: rect,
            size: fit(rect, long, short),
            zoomed: crop.is_some(),
        }
    }
}

/// Even dimensions that keep the crop's aspect, fit the long/short limits and never upscale.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "screen sizes are a few thousand pixels"
)]
fn fit(rect: Rect, long: f64, short: f64) -> (u32, u32) {
    let scale = (long / rect.w.max(rect.h))
        .min(short / rect.w.min(rect.h))
        .min(1.0);
    let even = |v: f64| ((v * scale).round() as u32 / 2 * 2).max(2);
    (even(rect.w), even(rect.h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn whole_display_is_capped_and_even() {
        assert_eq!(View::whole(1920, 1080).size, (1920, 1080));
        assert_eq!(View::whole(1001, 667).size, (1000, 666));
        // A 5K display shrinks to the 4K frame limit.
        let (w, h) = View::whole(5120, 2880).size;
        assert!(w <= 3840 && h <= 2160 && w % 2 == 0 && h % 2 == 0);
    }

    #[test]
    fn zoom_crops_and_fits_the_phone_viewport() {
        let view = View::apply(
            &json!({"crop": [100, 50, 960, 540], "long": 800, "short": 400}),
            1920,
            1080,
        );
        assert!(view.zoomed);
        assert_eq!(
            view.crop,
            Rect {
                x: 100.0,
                y: 50.0,
                w: 960.0,
                h: 540.0
            }
        );
        assert_eq!(view.size, (710, 400).into_even());
    }

    #[test]
    fn bad_crops_fall_back_to_the_whole_display() {
        let view = View::apply(&json!({"crop": [0, 0, 4, 4]}), 1280, 720);
        assert!(!view.zoomed);
        assert_eq!(view.size, (1280, 720));
        let clamped = View::apply(&json!({"crop": [1900, 1000, 500, 500]}), 1920, 1080);
        assert!(
            clamped.crop.x + clamped.crop.w <= 1920.0 && clamped.crop.y + clamped.crop.h <= 1080.0
        );
    }

    trait IntoEven {
        fn into_even(self) -> (u32, u32);
    }
    impl IntoEven for (u32, u32) {
        fn into_even(self) -> (u32, u32) {
            (self.0 / 2 * 2, self.1 / 2 * 2)
        }
    }
}
