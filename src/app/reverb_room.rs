// Copyright (C) 2026 ItsAlphaNeon
// SPDX-License-Identifier: GPL-3.0-or-later
// This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

//! The room visual: a wireframe prism turning in a tinted panel. The diffusion sets its number of sides, the room size
//! its shape and the camera's height, the wet level its glow; the panel's tint follows the filters, the decay, the high
//! damping and the bass multiplier. All laws are measured from the original's panel (160 px square); the panel is drawn
//! at any size by scaling that frame.
//!
// spec: plugins/lemon-reverb.md §2 (the room visual)

const LOW_CUT: usize = 0;
const HIGH_CUT: usize = 1;
const ROOM: usize = 3;
const DIFFUSION: usize = 4;
const DECAY: usize = 5;
const DAMPING: usize = 6;
const BASS: usize = 7;
const WET: usize = 12;
const PARAMS: usize = 15;

/// The model's frame: the original's panel in px.
pub const FRAME: f32 = 160.0;
/// Seconds per turn (counter-clockwise seen from above).
pub const TURN_SECS: f64 = 34.97;

/// Camera: distance from its aim, focal scale and the image of the aim (the panel's centre), in frame px.
const CAM_DIST: f32 = 3.0;
const FOCAL: f32 = 170.5;
const CENTRE: (f32, f32) = (80.0, 80.0);

/// The camera over the room size (fit per size; straight lines between): elevation in degrees above the horizon (90 =
/// straight down on the smallest room) and the prism's height, in units of a third of the camera's distance.
const CAMERA: [(i32, f32, f32); 20] = [
    (1, 90.0, 2.435),
    (2, 88.21, 2.328),
    (3, 86.51, 2.243),
    (5, 83.05, 2.084),
    (7, 79.78, 1.945),
    (10, 75.05, 1.775),
    (13, 70.36, 1.629),
    (16, 66.26, 1.52),
    (20, 61.1, 1.403),
    (25, 55.4, 1.295),
    (30, 50.32, 1.201),
    (35, 45.82, 1.13),
    (40, 41.74, 1.068),
    (45, 38.25, 1.018),
    (50, 35.07, 0.975),
    (60, 29.88, 0.906),
    (70, 25.81, 0.853),
    (80, 22.68, 0.814),
    (90, 20.22, 0.779),
    (100, 18.34, 0.751),
];

/// The prism's number of sides for diffusion 0..100.
pub const SIDES: [u8; 101] = [
    3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, //
    4, 4, 4, 4, 4, 4, 4, 4, 4, 5, 5, 5, 5, 5, 5, 5, 5, 5, 5, 6, //
    6, 6, 6, 6, 6, 6, 6, 7, 7, 7, 7, 7, 7, 8, 8, 8, 8, 8, 9, 9, //
    9, 9, 10, 10, 10, 10, 11, 11, 11, 11, 12, 12, 12, 13, 13, 14, 14, 14, 15, 15, //
    16, 16, 16, 17, 17, 18, 18, 19, 20, 20, 21, 21, 22, 23, 23, 24, 25, 26, 26, 27, //
    28,
];

/// Panel tint: the decay's strength over the decay knob (0..1).
const DECAY_STRENGTH: [(i32, f32); 15] = [
    (1, 0.09),
    (2, 0.12),
    (5, 0.152),
    (10, 0.212),
    (15, 0.27),
    (20, 0.333),
    (30, 0.45),
    (40, 0.515),
    (60, 0.66),
    (80, 0.758),
    (100, 0.82),
    (120, 0.879),
    (130, 0.9),
    (160, 0.94),
    (200, 1.0),
];

/// Line colour and width; the lines' opacity at the prism's axis and its gain towards the camera (per unit of a point's
/// horizontal distance in front of the axis, times the cosine of the elevation).
pub const LINE: [f32; 3] = [255.0, 255.0, 255.0];
pub const LINE_WIDTH: f32 = 1.15;
const SHADE_MID: f32 = 0.707;
const SHADE_GAIN: f32 = 0.342;
/// Glow per wet step (of the blurred, shaded line coverage) and its blur radius in frame px.
const GLOW_PER_WET: f32 = 0.0206;

fn interp(table: &[(i32, f32)], x: f32) -> f32 {
    let (first, last) = (table[0], table[table.len() - 1]);
    if x <= first.0 as f32 {
        return first.1;
    }
    for w in table.windows(2) {
        let ((x0, y0), (x1, y1)) = (w[0], w[1]);
        if x <= x1 as f32 {
            return y0 + (y1 - y0) * (x - x0 as f32) / (x1 - x0) as f32;
        }
    }
    last.1
}

/// The camera's elevation (degrees) and the prism's height at room size `size`, straight between the table's rows.
fn camera(size: f32) -> (f32, f32) {
    let i = CAMERA
        .iter()
        .position(|r| size <= r.0 as f32)
        .unwrap_or(CAMERA.len() - 1)
        .max(1);
    let ((x0, e0, h0), (x1, e1, h1)) = (CAMERA[i - 1], CAMERA[i]);
    let t = ((size - x0 as f32) / (x1 - x0) as f32).clamp(0.0, 1.0);
    (e0 + (e1 - e0) * t, h0 + (h1 - h0) * t)
}

/// The hue wheel at `h` degrees: red, yellow, green, cyan, blue, magenta, each channel 0..1 and linear between.
fn wheel(h: f32) -> [f32; 3] {
    let s = h.rem_euclid(360.0) / 60.0;
    let f = s.fract();
    match s as u32 % 6 {
        0 => [1.0, f, 0.0],
        1 => [1.0 - f, 1.0, 0.0],
        2 => [0.0, 1.0, f],
        3 => [0.0, 1.0 - f, 1.0],
        4 => [f, 0.0, 1.0],
        _ => [1.0, 0.0, 1.0 - f],
    }
}

/// What the visual shows for a setting.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub sides: u32,
    /// Prism radius and height, and how far its centre sits above the camera's aim.
    pub radius: f32,
    pub height: f32,
    pub lift: f32,
    /// Camera elevation in radians.
    pub elevation: f32,
    /// Panel colour, 0..255 per channel.
    pub tint: [f32; 3],
    /// Glow gain.
    pub glow: f32,
}

/// The panel colour: a grey whose brightness follows the bass multiplier, plus two hue-wheel colours (the low cut's,
/// and the high cut's scaled by the high damping) whose strength grows with the decay.
pub fn tint(k: &[i32; PARAMS]) -> [f32; 3] {
    let h1 = -0.0722 * (k[LOW_CUT].max(10) - 10) as f32;
    let h2 = 140.0 + k[HIGH_CUT] as f32;
    let damp = 4.6 + 0.0833 * (k[DAMPING] - 5) as f32;
    let s = interp(&DECAY_STRENGTH, k[DECAY] as f32);
    let grey = 83.5 - 0.085 * (k[BASS] - 100) as f32;
    let amp = (grey / 83.5).max(0.0).powf(0.75) * s;
    let (a, b) = (wheel(h1), wheel(h2));
    std::array::from_fn(|c| {
        (grey + amp * (42.0 * (a[c] - 0.5) + damp * (b[c] - 0.5))).clamp(0.0, 255.0)
    })
}

pub fn look(k: &[i32; PARAMS]) -> Look {
    let size = k[ROOM].clamp(1, 100) as f32;
    let (elevation, height) = camera(size);
    Look {
        sides: u32::from(SIDES[k[DIFFUSION].clamp(0, 100) as usize]),
        radius: 0.1655 + 0.00974 * size,
        height,
        lift: 0.0405 + 0.00109 * size,
        elevation: elevation.to_radians(),
        tint: tint(k),
        glow: GLOW_PER_WET * k[WET].max(0) as f32,
    }
}

/// The prism's edges in frame px with each end's line opacity, at turn angle `angle` (radians, counter-clockwise from
/// above; 0 = a corner towards the camera): the top rim, the bottom rim, the uprights.
pub fn edges(l: &Look, angle: f32) -> Vec<[(f32, f32, f32); 2]> {
    let n = l.sides.max(3);
    let (ce, se) = (l.elevation.cos(), l.elevation.sin());
    // Camera at distance D towards -y, raised by the elevation, looking at the origin; screen x = world x.
    let cam = [0.0, -CAM_DIST * ce, CAM_DIST * se];
    let fwd = [0.0, ce, -se];
    let up = [0.0, se, ce];
    let project = |p: [f32; 3]| {
        let v = [p[0] - cam[0], p[1] - cam[1], p[2] - cam[2]];
        let depth = (v[1] * fwd[1] + v[2] * fwd[2]).max(1e-3);
        let (x, y) = (v[0], v[1] * up[1] + v[2] * up[2]);
        // Lines fade with the horizontal distance behind the axis (the same at both ends of an upright).
        let shade = (SHADE_MID - SHADE_GAIN * p[1] * ce).clamp(0.0, 1.0);
        (
            CENTRE.0 + FOCAL * x / depth,
            CENTRE.1 - FOCAL * y / depth,
            shade,
        )
    };
    let corner = |i: u32, z: f32| {
        let t = angle - std::f32::consts::FRAC_PI_2 + std::f32::consts::TAU * i as f32 / n as f32;
        project([l.radius * t.cos(), l.radius * t.sin(), l.lift + z])
    };
    let top: Vec<_> = (0..n).map(|i| corner(i, 0.5 * l.height)).collect();
    let bot: Vec<_> = (0..n).map(|i| corner(i, -0.5 * l.height)).collect();
    let mut e = Vec::with_capacity(3 * n as usize);
    for i in 0..n as usize {
        let j = (i + 1) % n as usize;
        e.push([top[i], top[j]]);
        e.push([bot[i], bot[j]]);
        e.push([top[i], bot[i]]);
    }
    e
}

/// The turn angle `secs` after the editor opened.
pub fn angle_at(secs: f64) -> f32 {
    (std::f64::consts::TAU * (secs / TURN_SECS).fract()) as f32
}
