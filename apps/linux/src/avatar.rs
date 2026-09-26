//! Grok-Bot-style characters (same silhouettes and palette as the Apple apps), drawn with cairo.
//! Mirrors CodyncKit/Design/CharacterAvatar.swift: `CharacterAvatar`, `GroupAvatar`, `AvatarWithStatus`.

use gtk::cairo::Context;
use gtk::prelude::*;
use serde_json::Value;
use std::collections::HashMap;
use std::f64::consts::PI;

pub const SHAPES: &[&str] = &[
    "blob", "pebble", "squircle", "tablet", "wedge", "hex", "cloud", "teardrop",
];
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
    (
        f64::from((hex >> 16) & 0xFF) / 255.0,
        f64::from((hex >> 8) & 0xFF) / 255.0,
        f64::from(hex & 0xFF) / 255.0,
    )
}

pub fn color_of(name: &str) -> u32 {
    COLORS
        .iter()
        .find(|(n, _)| *n == name)
        .map_or(0x1084FE, |c| c.1)
}

fn rounded_rect(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 1.5 * PI);
    cr.close_path();
}

fn ellipse(cr: &Context, x: f64, y: f64, w: f64, h: f64) {
    cr.save().ok();
    cr.translate(x + w / 2.0, y + h / 2.0);
    cr.scale(w / 2.0, h / 2.0);
    cr.new_sub_path();
    cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI);
    cr.restore().ok();
}

/// A quadratic curve as the cubic cairo draws.
fn quad_to(cr: &Context, (cx, cy): (f64, f64), (x, y): (f64, f64)) {
    let (x0, y0) = cr.current_point().unwrap_or((x, y));
    cr.curve_to(
        x0 + 2.0 / 3.0 * (cx - x0),
        y0 + 2.0 / 3.0 * (cy - y0),
        x + 2.0 / 3.0 * (cx - x),
        y + 2.0 / 3.0 * (cy - y),
        x,
        y,
    );
}

/// Draws the silhouette into a 100×100 unit box (`CharacterShape`).
fn silhouette(cr: &Context, shape: &str) {
    match shape {
        "pebble" => ellipse(cr, 0.0, 10.0, 100.0, 80.0),
        "squircle" => rounded_rect(cr, 4.0, 4.0, 92.0, 92.0, 30.0),
        "tablet" => rounded_rect(cr, 14.0, 0.0, 72.0, 100.0, 22.0),
        "wedge" => {
            cr.move_to(50.0, 4.0);
            quad_to(cr, (90.0, 40.0), (98.0, 86.0));
            quad_to(cr, (50.0, 104.0), (2.0, 86.0));
            quad_to(cr, (10.0, 40.0), (50.0, 4.0));
            cr.close_path();
        }
        "hex" => {
            for i in 0..6 {
                let a = f64::from(i) * PI / 3.0 - PI / 2.0;
                let (x, y) = (50.0 + a.cos() * 49.0, 50.0 + a.sin() * 49.0);
                if i == 0 {
                    cr.move_to(x, y);
                } else {
                    cr.line_to(x, y);
                }
            }
            cr.close_path();
        }
        "cloud" => {
            ellipse(cr, 0.0, 30.0, 55.0, 55.0);
            ellipse(cr, 45.0, 30.0, 55.0, 55.0);
            ellipse(cr, 18.0, 8.0, 64.0, 64.0);
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
                let a = f64::from(i) / 64.0 * 2.0 * PI;
                let r = 46.0 + 3.5 * (a * 3.0 + 0.6).sin();
                let (x, y) = (50.0 + a.cos() * r, 50.0 + a.sin() * r);
                if i == 0 {
                    cr.move_to(x, y);
                } else {
                    cr.line_to(x, y);
                }
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

pub fn mood(b: &Value) -> Mood {
    match b["status"].as_str() {
        Some("needsInput") => Mood::Needs,
        Some("working") => Mood::Working,
        _ => Mood::Idle,
    }
}

/// Small icons use fewer, larger dots so the gaps and hollow eyes survive.
fn grid(size: f64) -> (i32, &'static [i32], &'static [i32]) {
    if size < 18.0 {
        (7, &[2, 4], &[2, 3])
    } else if size < 28.0 {
        (9, &[3, 6], &[3, 4])
    } else {
        (13, &[4, 8], &[4, 5, 6])
    }
}

/// A halftone of dots shaded as if the silhouette were a ball (grey ink, the bot's color
/// only on the brightest dots), eyes left hollow. Working swings the light and glances;
/// needing you sends a ripple out from the center.
pub fn draw(
    cr: &Context,
    size: f64,
    shape: &str,
    color: &str,
    ink: (f64, f64, f64),
    mood: Mood,
    t: f64,
) {
    cr.save().ok();
    cr.scale(size / 100.0, size / 100.0);
    let (r, g, b) = rgb(color_of(color));
    let (cells, eye_cols, all_eye_rows) = grid(size);
    let step = 100.0 / f64::from(cells);
    let animated = mood != Mood::Idle;
    let glance = if mood == Mood::Working {
        ((t * 2.0 * PI / 3.2).sin() * 1.4).round() as i32
    } else {
        0
    };
    let blinking = animated && (t / 4.7).fract() < 0.035;
    let eye_rows = if blinking {
        &all_eye_rows[all_eye_rows.len() - 1..]
    } else {
        all_eye_rows
    };
    silhouette(cr, shape);
    let hex = shape == "hex";
    if hex {
        cr.set_line_width(8.0);
        cr.set_line_join(gtk::cairo::LineJoin::Round);
    }
    let mut dots = Vec::new();
    for row in 0..cells {
        for col in 0..cells {
            let (x, y) = ((f64::from(col) + 0.5) * step, (f64::from(row) + 0.5) * step);
            let inside =
                cr.in_fill(x, y).unwrap_or(false) || (hex && cr.in_stroke(x, y).unwrap_or(false));
            let eye = eye_cols.iter().any(|c| c + glance == col) && eye_rows.contains(&row);
            if inside && !eye {
                dots.push((x, y));
            }
        }
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
        cr.arc(x, y, step * 0.42 * (0.55 + 0.45 * shade), 0.0, 2.0 * PI);
        let alpha = if cells < 13 {
            0.4 + 0.4 * shade
        } else {
            0.2 + 0.4 * (shade / 0.7).min(1.0)
        };
        cr.set_source_rgba(ink.0, ink.1, ink.2, alpha);
        cr.fill_preserve().ok();
        if shade > 0.6 {
            cr.set_source_rgba(r, g, b, (shade - 0.6) / 0.4);
            cr.fill_preserve().ok();
        }
        cr.new_path();
    }
    cr.restore().ok();
}

type Part = (String, String, Mood);

fn part(b: &Value, animated: bool) -> Part {
    (
        b["avatarShape"].as_str().unwrap_or("blob").to_owned(),
        b["avatarColor"].as_str().unwrap_or("blue").to_owned(),
        if animated { mood(b) } else { Mood::Idle },
    )
}

#[derive(Clone, Copy, PartialEq)]
enum Badge {
    None,
    Unread,
    Needs,
}

/// One character in a fixed look.
pub fn shape(shape: &str, color: &str, size: i32, mood: Mood) -> gtk::DrawingArea {
    area(
        vec![(shape.to_owned(), color.to_owned(), mood)],
        size,
        Badge::None,
    )
}

/// A bot's character, or for a group its first two members, one tucked behind the other.
pub fn of(bots: &HashMap<String, Value>, b: &Value, size: i32, animated: bool) -> gtk::DrawingArea {
    build(bots, b, size, animated, Badge::None)
}

/// The roster avatar: a dot when unread, an amber "!" when it needs you.
pub fn with_status(bots: &HashMap<String, Value>, b: &Value, size: i32) -> gtk::DrawingArea {
    let badge = if b["status"] == "needsInput" {
        Badge::Needs
    } else if b["unread"].as_i64().unwrap_or(0) > 0 {
        Badge::Unread
    } else {
        Badge::None
    };
    build(bots, b, size, true, badge)
}

fn build(
    bots: &HashMap<String, Value>,
    b: &Value,
    size: i32,
    animated: bool,
    badge: Badge,
) -> gtk::DrawingArea {
    if b["kind"] == "group" {
        let parts = b["members"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|m| bots.get(m.as_str()?))
            .take(2)
            .map(|m| part(m, animated))
            .collect();
        area(parts, size, badge)
    } else {
        area(vec![part(b, animated)], size, badge)
    }
}

/// Several characters drawn side by side, overlapping by `overlap` px (the empty state's trio).
pub fn row(looks: &[(&str, &str, Mood)], size: i32, overlap: i32) -> gtk::DrawingArea {
    let n = i32::try_from(looks.len()).unwrap_or(1);
    let looks: Vec<Part> = looks
        .iter()
        .map(|(s, c, m)| ((*s).to_owned(), (*c).to_owned(), *m))
        .collect();
    let moving = looks.iter().any(|p| p.2 != Mood::Idle);
    let area = gtk::DrawingArea::builder()
        .content_width(size * n - overlap * (n - 1))
        .content_height(size)
        .halign(gtk::Align::Center)
        .build();
    let start = std::time::Instant::now();
    area.set_draw_func(move |area, cr, _, _| {
        let fg = area.color();
        let ink = (
            f64::from(fg.red()),
            f64::from(fg.green()),
            f64::from(fg.blue()),
        );
        let t = start.elapsed().as_secs_f64();
        for (i, (s, c, m)) in looks.iter().enumerate() {
            cr.save().ok();
            cr.translate(f64::from(size - overlap) * i as f64, 0.0);
            draw(cr, f64::from(size), s, c, ink, *m, t);
            cr.restore().ok();
        }
    });
    if moving {
        area.add_tick_callback(|a, _| {
            a.queue_draw();
            gtk::glib::ControlFlow::Continue
        });
    }
    area
}

fn area(parts: Vec<Part>, size: i32, badge: Badge) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(size)
        .content_height(size)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .build();
    let moving = parts.iter().any(|p| p.2 != Mood::Idle);
    let start = std::time::Instant::now();
    area.set_draw_func(move |area, cr, w, _| {
        let s = f64::from(w);
        let fg = area.color();
        let ink = (
            f64::from(fg.red()),
            f64::from(fg.green()),
            f64::from(fg.blue()),
        );
        let t = start.elapsed().as_secs_f64();
        match parts.as_slice() {
            [] => draw(cr, s, "blob", "gray", ink, Mood::Idle, t),
            [(shape, color, m)] => draw(cr, s, shape, color, ink, *m, t),
            [first, second, ..] => {
                let small = s * 0.66;
                // Second member top-right, behind; first bottom-left.
                for (p, (x, y)) in [(second, (s - small, 0.0)), (first, (0.0, s - small))] {
                    cr.save().ok();
                    cr.translate(x, y);
                    draw(cr, small, &p.0, &p.1, ink, p.2, t);
                    cr.restore().ok();
                }
            }
        }
        if badge != Badge::None {
            let d = s * if badge == Badge::Needs { 0.36 } else { 0.28 };
            let (cx, cy) = (s - d / 2.0, s - d / 2.0);
            let dark = adw::StyleManager::default().is_dark();
            let bg = if dark { 0.04 } else { 1.0 };
            cr.arc(cx, cy, d / 2.0 + 2.0, 0.0, 2.0 * PI);
            cr.set_source_rgb(bg, bg, bg);
            cr.fill().ok();
            cr.arc(cx, cy, d / 2.0, 0.0, 2.0 * PI);
            if badge == Badge::Needs {
                let (r, g, b) = rgb(0xF0A030);
                cr.set_source_rgb(r, g, b);
                cr.fill().ok();
                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.set_line_width(d * 0.16);
                cr.set_line_cap(gtk::cairo::LineCap::Round);
                cr.move_to(cx, cy - d * 0.22);
                cr.line_to(cx, cy + d * 0.06);
                cr.stroke().ok();
                cr.arc(cx, cy + d * 0.24, d * 0.08, 0.0, 2.0 * PI);
                cr.fill().ok();
            } else {
                cr.set_source_rgb(ink.0, ink.1, ink.2);
                cr.fill().ok();
            }
        }
    });
    if moving {
        area.add_tick_callback(|a, _| {
            a.queue_draw();
            gtk::glib::ControlFlow::Continue
        });
    }
    area
}
