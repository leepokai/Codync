//! `ThinkingOrb` from the Apple apps (CodyncKit/Design/ThinkingOrbGeometry.swift, a port of
//! thinking-orbs 0.3.1, MIT, Jakub Antalik): the *working* orbits and the *listening* lattice.
//! Drawn in the widget's CSS color, so a `warning-text` / `secondary` class tints it.

use gtk::prelude::*;
use std::f64::consts::PI;

struct Dot {
    x: f64,
    y: f64,
    z: f64,
    radius: f64,
    white: f64,
    alpha: f64,
}

fn hash(a: f64, b: f64) -> f64 {
    let h = (a * 12.9898 + b * 78.233).sin() * 43758.5453;
    h - h.floor()
}

type P = (f64, f64, f64);

fn projector(yaw: f64, tilt: f64, size: f64, scale: f64) -> impl Fn(P) -> P {
    let (sy, cy, st, ct) = (yaw.sin(), yaw.cos(), tilt.sin(), tilt.cos());
    move |(px, py, pz)| {
        let x = px * cy + pz * sy;
        let z = -px * sy + pz * cy;
        (
            size / 2.0 + x * scale,
            size / 2.0 - (py * ct - z * st) * scale,
            py * st + z * ct,
        )
    }
}

fn orbits(size: f64, t: f64) -> Vec<Dot> {
    let small = size < 40.0;
    let radius = size / 2.0 * 0.82;
    let project = projector(t * 0.12, 0.3, size, 1.0);
    let rs = (size / 300.0).powf(0.6);
    let multiplier = if small { 2.4 } else { 1.0 };
    let (orbit_count, ghost_count) = if small { (3, 10) } else { (12, 40) };
    let mut dots = vec![];
    for orb in 0..orbit_count {
        let o = f64::from(orb);
        let (h1, h2, h3) = (hash(o, 1.7), hash(o, 5.2), hash(o, 8.9));
        let ro = radius * (0.45 + 0.52 * h1);
        let (theta, phi) = (h1 * 2.0 * PI, (2.0 * h2 - 1.0).acos());
        let n = (phi.sin() * theta.cos(), phi.cos(), phi.sin() * theta.sin());
        let mut u = (-n.1, n.0, 0.0);
        let len = (u.0 * u.0 + u.1 * u.1).sqrt().max(1e-6);
        u = (u.0 / len, u.1 / len, 0.0);
        let v = (-n.2 * u.1, n.2 * u.0, n.0 * u.1 - n.1 * u.0);
        let speed = (0.25 + 0.55 * h3) * if h3 > 0.5 { 1.0 } else { -1.0 };
        let at = |angle: f64| {
            project((
                (u.0 * angle.cos() + v.0 * angle.sin()) * ro,
                (u.1 * angle.cos() + v.1 * angle.sin()) * ro,
                (u.2 * angle.cos() + v.2 * angle.sin()) * ro,
            ))
        };
        for k in 0..ghost_count {
            let p = at(f64::from(k) / f64::from(ghost_count) * 2.0 * PI);
            let depth = (p.2 / ro + 1.0) / 2.0;
            dots.push(Dot {
                x: p.0,
                y: p.1,
                z: p.2,
                radius: 0.9 * multiplier * rs,
                white: 0.72,
                alpha: 0.5 * (0.4 + 0.6 * depth),
            });
        }
        for m in 0..3 {
            let p = at(t * speed + f64::from(m) / 3.0 * 2.0 * PI + h2 * 6.0);
            let depth = (p.2 / ro + 1.0) / 2.0;
            dots.push(Dot {
                x: p.0,
                y: p.1,
                z: p.2,
                radius: (1.2 + 1.6 * depth) * multiplier * rs,
                white: 0.3 - 0.22 * depth,
                alpha: 1.0,
            });
        }
    }
    dots
}

fn listening(size: f64, t: f64) -> Vec<Dot> {
    let small = size < 40.0;
    let rs = (size / 300.0).powf(0.6);
    let rings = if small { 5 } else { 9 };
    let density = if small { 13.0 } else { 23.0 };
    let multiplier = if small { 1.6 } else { 1.0 };
    let radius = size / 2.0 * 0.874;
    let project = projector(t * 0.18, 0.38, size, 1.0);
    let mut dots = vec![];
    for ri in 0..=rings {
        let r = f64::from(ri);
        let lat = -PI / 2.0 + r / f64::from(rings) * PI;
        let wave = 0.62 * (t * 2.1 - r * 0.52).sin() + 0.38 * (t * 1.27 + r * 0.83).sin();
        let rr = radius * (0.88 + 0.105 * wave);
        let lon_count = ((lat.cos().abs() * density).round() as i32).max(1);
        for j in 0..lon_count {
            let lon = f64::from(j) / f64::from(lon_count) * 2.0 * PI;
            let p = project((
                lat.cos() * lon.cos() * rr,
                lat.sin() * rr,
                lat.cos() * lon.sin() * rr,
            ));
            let depth = (p.2 / radius + 1.0) / 2.0;
            let crest = wave.max(0.0);
            dots.push(Dot {
                x: p.0,
                y: p.1,
                z: p.2,
                radius: (0.6 + 1.7 * depth) * multiplier * (1.0 + 0.4 * crest) * rs,
                white: 0.66 - 0.56 * depth - 0.1 * crest,
                alpha: 1.0,
            });
        }
    }
    dots
}

/// A turning orb; `listening` is the "needs you" state.
pub fn widget(listening_state: bool, size: i32) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .content_width(size)
        .content_height(size)
        .valign(gtk::Align::Center)
        .build();
    let start = std::time::Instant::now();
    area.set_draw_func(move |area, cr, w, _| {
        let s = f64::from(w);
        let c = area.color();
        let speed = if listening_state { 3.998 } else { 3.9 };
        let t = 0.6 + start.elapsed().as_secs_f64() * speed;
        let mut dots = if listening_state {
            listening(s, t)
        } else {
            orbits(s, t)
        };
        dots.sort_by(|a, b| a.z.total_cmp(&b.z));
        for d in dots.iter().filter(|d| d.alpha >= 0.02) {
            let ink = (1.0 - d.white.clamp(0.0, 1.0)) * d.alpha;
            cr.set_source_rgba(
                f64::from(c.red()),
                f64::from(c.green()),
                f64::from(c.blue()),
                ink,
            );
            cr.arc(d.x, d.y, d.radius.max(0.3), 0.0, 2.0 * PI);
            cr.fill().ok();
        }
    });
    area.add_tick_callback(|a, _| {
        a.queue_draw();
        gtk::glib::ControlFlow::Continue
    });
    area
}
