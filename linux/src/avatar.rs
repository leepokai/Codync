//! Grok-Bot-style characters (same silhouettes and palette as the Apple apps), drawn with cairo.

use gtk::cairo::Context;
use gtk::prelude::*;
use std::f64::consts::PI;

pub const SHAPES: &[&str] = &["blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop"];
pub const COLORS: &[(&str, u32)] = &[
    ("black", 0x2B2B2B),
    ("brown", 0x936439),
    ("red", 0xFF263C),
    ("orange", 0xFF6700),
    ("yellow", 0xFF9800),
    ("green", 0x00C972),
    ("cyan", 0x00BCA6),
    ("blue", 0x1084FE),
    ("violet", 0x9159FE),
    ("magenta", 0xFF309B),
    ("gray", 0x777777),
];

pub fn rgb(hex: u32) -> (f64, f64, f64) {
    (((hex >> 16) & 0xFF) as f64 / 255.0, ((hex >> 8) & 0xFF) as f64 / 255.0, (hex & 0xFF) as f64 / 255.0)
}

fn color_of(name: &str) -> u32 {
    COLORS.iter().find(|(n, _)| *n == name).map(|c| c.1).unwrap_or(0x1084FE)
}

fn rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

/// Draws the silhouette into a 100×100 unit box.
fn silhouette(cr: &Context, shape: &str) {
    match shape {
        "pebble" => {
            cr.save().ok();
            cr.translate(50.0, 50.0);
            cr.scale(50.0, 40.0);
            cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI);
            cr.restore().ok();
        }
        "squircle" => rounded_rect(cr, 4.0, 4.0, 92.0, 92.0, 30.0),
        "tablet" => rounded_rect(cr, 14.0, 0.0, 72.0, 100.0, 22.0),
        "wedge" => {
            cr.move_to(50.0, 4.0);
            cr.curve_to(76.0, 30.0, 90.0, 55.0, 98.0, 86.0);
            cr.curve_to(66.0, 100.0, 34.0, 100.0, 2.0, 86.0);
            cr.curve_to(10.0, 55.0, 24.0, 30.0, 50.0, 4.0);
            cr.close_path();
        }
        "hex" => {
            for i in 0..6 {
                let a = i as f64 * PI / 3.0 - PI / 2.0;
                let (x, y) = (50.0 + a.cos() * 49.0, 50.0 + a.sin() * 49.0);
                if i == 0 { cr.move_to(x, y) } else { cr.line_to(x, y) }
            }
            cr.close_path();
        }
        "cloud" => {
            for (x, y, r) in [(27.0, 57.0, 27.0), (73.0, 57.0, 27.0), (50.0, 40.0, 32.0)] {
                cr.new_sub_path();
                cr.arc(x, y, r, 0.0, 2.0 * PI);
            }
            rounded_rect(cr, 10.0, 50.0, 80.0, 35.0, 15.0);
        }
        "teardrop" => {
            cr.move_to(50.0, 0.0);
            cr.curve_to(62.0, 20.0, 94.0, 38.0, 94.0, 62.0);
            cr.arc(50.0, 62.0, 44.0, 0.0, PI);
            cr.curve_to(6.0, 38.0, 38.0, 20.0, 50.0, 0.0);
            cr.close_path();
        }
        _ => {
            for i in 0..=64 {
                let a = i as f64 / 64.0 * 2.0 * PI;
                let r = 46.0 + 3.5 * (a * 3.0 + 0.6).sin();
                let (x, y) = (50.0 + a.cos() * r, 50.0 + a.sin() * r);
                if i == 0 { cr.move_to(x, y) } else { cr.line_to(x, y) }
            }
            cr.close_path();
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Mood {
    Idle,
    Working,
    Needs,
}

/// Same look as `CharacterAvatar` on Apple platforms: a halftone of dots shaded
/// as if the silhouette were a sphere (grey ink, the bot's color only on the
/// brightest dots) and two slit eyes. Working swings the light and drifts the
/// eyes; needing you sends a ripple out from the center. Below 24px the body
/// stays solid because the dots stop reading.
pub fn draw(cr: &Context, size: f64, shape: &str, color: &str, ink: (f64, f64, f64), mood: Mood, t: f64) {
    cr.save().ok();
    cr.scale(size / 100.0, size / 100.0);
    let (r, g, b) = rgb(color_of(color));
    silhouette(cr, shape);
    if size < 24.0 {
        cr.set_source_rgb(r, g, b);
        cr.fill().ok();
    } else {
        let step = 100.0 / 16.0;
        let row_h = step * 0.866;
        let mut dots = Vec::new();
        let (mut y, mut row) = (row_h / 2.0, 0);
        while y < 100.0 {
            let mut x = if row % 2 == 0 { step / 2.0 } else { step };
            while x < 100.0 {
                if cr.in_fill(x, y).unwrap_or(false) {
                    dots.push((x, y));
                }
                x += step;
            }
            y += row_h;
            row += 1;
        }
        cr.new_path();
        let yaw = if mood == Mood::Working { t * 1.4 } else { -0.7 };
        let (lx, ly, lz) = (yaw.sin() * 0.8, 0.55, yaw.cos() * 0.5 + 0.6); // never fully behind
        let ll = (lx * lx + ly * ly + lz * lz).sqrt();
        for (x, y) in dots {
            let (u, v) = ((x - 50.0) / 50.0, (50.0 - y) / 50.0);
            let z = (1.0 - u * u - v * v).max(0.2).sqrt();
            let nl = (u * u + v * v + z * z).sqrt();
            let mut shade = 0.3 + 0.7 * ((u * lx + v * ly + z * lz) / (nl * ll)).max(0.0);
            if mood == Mood::Needs {
                shade *= 0.6 + 0.4 * (0.5 + 0.5 * ((u * u + v * v).sqrt() * 9.0 - t * 5.0).sin());
            }
            cr.arc(x, y, step * 0.38 * (0.3 + 0.7 * shade), 0.0, 2.0 * PI);
            cr.set_source_rgba(ink.0, ink.1, ink.2, 0.12 + 0.33 * (shade / 0.7).min(1.0));
            cr.fill_preserve().ok();
            if shade > 0.7 {
                cr.set_source_rgba(r, g, b, (shade - 0.7) / 0.3);
                cr.fill_preserve().ok();
            }
            cr.new_path();
        }
    }
    let dx = if mood == Mood::Working { (t * 2.0 * PI / 3.2).sin() * 4.0 } else { 0.0 };
    let phase = (t / 4.7).fract();
    let blink = if mood != Mood::Idle && phase < 0.04 { ((phase - 0.02).abs() / 0.02).max(0.12) } else { 1.0 };
    let (w, h) = ((1.4 * 100.0 / size).max(3.8), 12.0 * blink);
    for side in [-1.0, 1.0] {
        rounded_rect(cr, 50.0 + side * 10.0 + dx - w / 2.0, 47.0 - h / 2.0, w, h, w.min(h) / 2.0);
    }
    cr.set_source_rgb(ink.0, ink.1, ink.2);
    cr.fill().ok();
    cr.restore().ok();
}

/// status: "" | "unread" | "needs"
pub fn widget(shape: &str, color: &str, size: i32, working: bool, status: &str) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder().content_width(size).content_height(size).valign(gtk::Align::Center).build();
    let (shape, color, status) = (shape.to_owned(), color.to_owned(), status.to_owned());
    let mood = if status == "needs" { Mood::Needs } else if working { Mood::Working } else { Mood::Idle };
    let start = std::time::Instant::now();
    area.set_draw_func(move |area, cr, w, _| {
        let s = w as f64;
        let fg = area.color();
        let ink = (fg.red() as f64, fg.green() as f64, fg.blue() as f64);
        draw(cr, s, &shape, &color, ink, mood, start.elapsed().as_secs_f64());
        if !status.is_empty() {
            let d = s * if status == "needs" { 0.36 } else { 0.28 };
            let (cx, cy) = (s - d / 2.0, s - d / 2.0);
            let (r, g, b) = if status == "needs" { rgb(0xF0A030) } else { ink };
            cr.arc(cx, cy, d / 2.0, 0.0, 2.0 * PI);
            cr.set_source_rgb(r, g, b);
            cr.fill().ok();
        }
    });
    if working {
        area.add_tick_callback(|a, _| {
            a.queue_draw();
            gtk::glib::ControlFlow::Continue
        });
    }
    area
}
