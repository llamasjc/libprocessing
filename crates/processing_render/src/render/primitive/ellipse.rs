use bevy::prelude::*;
use lyon::{geom::Point, path::Path};

use crate::render::primitive::{StrokeConfig, TessellationMode, tessellate_path};

// magic number for cubic bezier approximation of a quarter circle.
// kappa = 4 * (sqrt(2) - 1) / 3
const KAPPA: f32 = 0.552_284_8;

fn ellipse_path(cx: f32, cy: f32, w: f32, h: f32) -> Path {
    let rx = w / 2.0;
    let ry = h / 2.0;
    let kx = rx * KAPPA;
    let ky = ry * KAPPA;

    let mut b = Path::builder();

    b.begin(Point::new(cx, cy - ry));

    // rc -> rc
    b.cubic_bezier_to(
        Point::new(cx + kx, cy - ry),
        Point::new(cx + rx, cy - ky),
        Point::new(cx + rx, cy),
    );
    // rc -> bc
    b.cubic_bezier_to(
        Point::new(cx + rx, cy + ky),
        Point::new(cx + kx, cy + ry),
        Point::new(cx, cy + ry),
    );
    // bc -> lc
    b.cubic_bezier_to(
        Point::new(cx - kx, cy + ry),
        Point::new(cx - rx, cy + ky),
        Point::new(cx - rx, cy),
    );
    // lc -> tc
    b.cubic_bezier_to(
        Point::new(cx - rx, cy - ky),
        Point::new(cx - kx, cy - ry),
        Point::new(cx, cy - ry),
    );

    b.end(true);
    b.build()
}

pub fn ellipse(
    mesh: &mut Mesh,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    color: Color,
    mode: TessellationMode,
    stroke_config: &StrokeConfig,
) {
    if fast_ellipse(mesh, cx, cy, w, h, color, &mode) {
        return;
    }
    let path = ellipse_path(cx, cy, w, h);
    tessellate_path(mesh, &path, color, mode, stroke_config);
}


/// An ellipse is convex and its outline has no corners, so its fill is a fan
/// and its stroke a ring: written straight into the mesh, without lyon's
/// general tessellators (several times faster for the many small ellipses a
/// sketch draws). false for the cases left to lyon.
fn fast_ellipse(
    mesh: &mut Mesh,
    cx: f32,
    cy: f32,
    w: f32,
    h: f32,
    color: Color,
    mode: &TessellationMode,
) -> bool {
    let (rx, ry) = (w.abs() / 2.0, h.abs() / 2.0);
    if !(rx > 0.0 && ry > 0.0) || !rx.is_finite() || !ry.is_finite() {
        return false;
    }
    let mut pos: Vec<[f32; 3]> = Vec::new();
    let mut idx: Vec<u32> = Vec::new();
    match mode {
        TessellationMode::Fill => {
            let n = super::segments(rx.max(ry));
            pos.reserve(n + 1);
            pos.push([cx, cy, 0.0]);
            for i in 0..n {
                let t = std::f32::consts::TAU * i as f32 / n as f32;
                pos.push([cx + rx * t.cos(), cy + ry * t.sin(), 0.0]);
            }
            for i in 0..n as u32 {
                idx.extend_from_slice(&[0, 1 + i, 1 + (i + 1) % n as u32]);
            }
        }
        TessellationMode::Stroke(weight) => {
            let weight = *weight;
            if !(weight > 0.0) || !weight.is_finite() {
                return false;
            }
            let half = weight / 2.0;
            let n = super::segments(rx.max(ry) + half);
            pos.reserve(2 * n);
            for i in 0..n {
                let t = std::f32::consts::TAU * i as f32 / n as f32;
                let (c, s) = (t.cos(), t.sin());
                // offset along the ellipse's normal, like a stroke does
                let (nx, ny) = (c * ry, s * rx);
                let l = (nx * nx + ny * ny).sqrt().max(1e-6);
                let (nx, ny) = (nx / l * half, ny / l * half);
                let (px, py) = (cx + rx * c, cy + ry * s);
                pos.push([px + nx, py + ny, 0.0]);
                pos.push([px - nx, py - ny, 0.0]);
            }
            for i in 0..n as u32 {
                let j = (i + 1) % n as u32;
                let (a, b, c, d) = (2 * i, 2 * i + 1, 2 * j, 2 * j + 1);
                idx.extend_from_slice(&[a, b, c, b, d, c]);
            }
        }
    }
    super::push_flat(mesh, &pos, &idx, color)
}
