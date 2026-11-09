use bevy::prelude::*;
use lyon::{geom::Point, path::Path};

use crate::render::primitive::{StrokeConfig, TessellationMode, tessellate_path};

pub fn line(
    mesh: &mut Mesh,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: Color,
    weight: f32,
    stroke_config: &StrokeConfig,
) {
    if fast_line(mesh, x1, y1, x2, y2, color, weight, stroke_config) {
        return;
    }
    let mut builder = Path::builder();
    builder.begin(Point::new(x1, y1));
    builder.line_to(Point::new(x2, y2));
    builder.end(false);
    let path = builder.build();
    tessellate_path(
        mesh,
        &path,
        color,
        TessellationMode::Stroke(weight),
        stroke_config,
    );
}

/// A straight line is a quad plus its caps, written straight into the mesh
/// (lyon's stroke tessellator is general and much slower per line). false
/// for the cases left to lyon.
#[allow(clippy::too_many_arguments)]
fn fast_line(
    mesh: &mut Mesh,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    color: Color,
    weight: f32,
    stroke_config: &StrokeConfig,
) -> bool {
    use crate::render::command::StrokeCapMode;
    let (dx, dy) = (x2 - x1, y2 - y1);
    let len = (dx * dx + dy * dy).sqrt();
    if !(weight > 0.0) || !(len > 0.0) || !len.is_finite() || !weight.is_finite() {
        return false;
    }
    let half = weight / 2.0;
    let (ux, uy) = (dx / len, dy / len);
    let (nx, ny) = (-uy * half, ux * half);
    // PROJECT goes past the ends by half the weight
    let ext = if matches!(stroke_config.line_cap, StrokeCapMode::Project) { half } else { 0.0 };
    let (ax, ay) = (x1 - ux * ext, y1 - uy * ext);
    let (bx, by) = (x2 + ux * ext, y2 + uy * ext);
    let mut pos = vec![
        [ax + nx, ay + ny, 0.0],
        [ax - nx, ay - ny, 0.0],
        [bx + nx, by + ny, 0.0],
        [bx - nx, by - ny, 0.0],
    ];
    let mut idx = vec![0, 1, 2, 1, 3, 2];
    if matches!(stroke_config.line_cap, StrokeCapMode::Round) {
        // a half disc on each end, outside the quad
        let k = (super::segments(half) / 2).max(4);
        for (cx, cy, sx, sy) in [(x1, y1, -1.0f32, 1.0f32), (x2, y2, 1.0, -1.0)] {
            let c = pos.len() as u32;
            pos.push([cx, cy, 0.0]);
            // from +n round the back (start) or the front (end) to -n
            for i in 0..=k {
                let t = std::f32::consts::PI * i as f32 / k as f32;
                let (ct, st) = (t.cos(), t.sin());
                let along = st * sx;
                let side = ct * sy;
                pos.push([cx + ux * along * half + nx * side, cy + uy * along * half + ny * side, 0.0]);
            }
            for i in 0..k as u32 {
                idx.extend_from_slice(&[c, c + 1 + i, c + 2 + i]);
            }
        }
    }
    super::push_flat(mesh, &pos, &idx, color)
}
