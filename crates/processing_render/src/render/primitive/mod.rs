mod arc;
mod curves;
mod ellipse;
mod line;
mod quad;
mod rect;
pub(crate) mod shape;
mod shape3d;
pub mod text;
mod triangle;

pub use arc::{arc_fill, arc_stroke};
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
pub use curves::{bezier, curve};
pub use ellipse::ellipse;
pub use line::line;
use lyon::{
    path::Path,
    tessellation::{
        FillOptions, FillTessellator, LineCap, LineJoin, StrokeOptions, StrokeTessellator,
    },
};
pub use quad::quad;
pub use rect::rect;
pub use shape::{
    ShapeBuilder, VertexType, build_direct_fill, build_direct_stroke, build_polygon_fill,
    build_polygon_stroke,
};
pub use shape3d::{
    box_mesh, capsule_mesh, cone_mesh, conical_frustum_mesh, cylinder_mesh, grid_mesh, plane_mesh,
    sphere_mesh, tetrahedron_mesh, torus_mesh,
};
pub use triangle::triangle;

use super::command::{StrokeCapMode, StrokeJoinMode};
use super::mesh_builder::MeshBuilder;

pub enum TessellationMode {
    Fill,
    Stroke(f32),
}

#[derive(Debug, Clone, Copy)]
pub struct StrokeConfig {
    pub line_cap: StrokeCapMode,
    pub line_join: StrokeJoinMode,
}

impl Default for StrokeConfig {
    fn default() -> Self {
        Self {
            line_cap: StrokeCapMode::Round,
            line_join: StrokeJoinMode::Round,
        }
    }
}

impl StrokeCapMode {
    pub fn to_lyon(self) -> LineCap {
        match self {
            Self::Round => LineCap::Round,
            // lyon's naming is swapped vs processing:
            // processing SQUARE = flush at endpoint = lyon Butt
            // processing PROJECT = extends past endpoint = lyon Square
            Self::Square => LineCap::Butt,
            Self::Project => LineCap::Square,
        }
    }
}

impl StrokeJoinMode {
    pub fn to_lyon(self) -> LineJoin {
        match self {
            Self::Round => LineJoin::Round,
            Self::Miter => LineJoin::Miter,
            Self::Bevel => LineJoin::Bevel,
        }
    }
}

/// How many straight pieces keep a circle of radius r within lyon's default
/// tolerance (0.1) of the true curve.
pub(crate) fn segments(r: f32) -> usize {
    let r = r.max(0.5);
    let step = 2.0 * (1.0 - (0.1 / r).min(1.0)).acos();
    ((std::f32::consts::TAU / step).ceil() as usize).clamp(8, 512)
}

/// Appends flat 2D triangles (indices count from the first of `pos`) in one
/// go per attribute; false if the mesh isn't one of ours.
pub(crate) fn push_flat(mesh: &mut Mesh, pos: &[[f32; 3]], idx: &[u32], color: Color) -> bool {
    use bevy::mesh::{Indices, VertexAttributeValues};
    let base = match mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        Some(VertexAttributeValues::Float32x3(p)) => p.len() as u32,
        _ => return false,
    };
    let count = pos.len();
    let rgba = color.to_linear().to_f32_array();
    if let Some(VertexAttributeValues::Float32x3(p)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
        p.extend_from_slice(pos);
    }
    if let Some(VertexAttributeValues::Float32x4(c)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
        c.extend(std::iter::repeat_n(rgba, count));
    }
    if let Some(VertexAttributeValues::Float32x3(n)) = mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL) {
        n.extend(std::iter::repeat_n([0.0, 0.0, 1.0], count));
    }
    if let Some(VertexAttributeValues::Float32x2(u)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        u.extend(std::iter::repeat_n([0.0, 0.0], count));
    }
    if let Some(Indices::U32(i)) = mesh.indices_mut() {
        i.extend(idx.iter().map(|k| k + base));
    }
    true
}

/// The outline of a convex polygon (rect, triangle, quad) with MITER joins,
/// as a ring of quads written straight into the mesh: each corner's outer and
/// inner points are where the offset edges meet. false (and nothing written)
/// for the cases left to lyon: other joins, concave or degenerate shapes, a
/// corner sharper than lyon's miter limit, or a weight that swallows the
/// inside.
pub(crate) fn fast_closed_stroke(
    mesh: &mut Mesh,
    pts: &[(f32, f32)],
    color: Color,
    weight: f32,
    stroke_config: &StrokeConfig,
) -> bool {
    use crate::render::command::StrokeJoinMode;
    let n = pts.len();
    if n < 3 || !matches!(stroke_config.line_join, StrokeJoinMode::Miter) {
        return false;
    }
    if !(weight > 0.0) || !weight.is_finite() || pts.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
        return false;
    }
    let half = weight / 2.0;
    // which way round the points go, and that every corner turns that way
    let mut area = 0.0;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        area += a.0 * b.1 - b.0 * a.1;
    }
    if area.abs() < 1e-6 {
        return false;
    }
    let s = area.signum();
    let mut normals = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b, c) = (pts[i], pts[(i + 1) % n], pts[(i + 2) % n]);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if !(len > 1e-6) {
            return false;
        }
        let turn = dx * (c.1 - b.1) - dy * (c.0 - b.0);
        if turn * s < 0.0 {
            return false;
        }
        normals.push((s * dy / len, -s * dx / len));
    }
    let mut pos = Vec::with_capacity(2 * n);
    for i in 0..n {
        let (n0, n1) = (normals[(i + n - 1) % n], normals[i]);
        let (mx, my) = (n0.0 + n1.0, n0.1 + n1.1);
        let ml = (mx * mx + my * my).sqrt();
        if !(ml > 1e-6) {
            return false;
        }
        let (mx, my) = (mx / ml, my / ml);
        let cos = mx * n1.0 + my * n1.1;
        // lyon's default miter limit is 4: sharper corners get a bevel there
        if !(cos > 0.25) {
            return false;
        }
        let (vx, vy) = (mx * half / cos, my * half / cos);
        let p = pts[i];
        pos.push([p.0 + vx, p.1 + vy, 0.0]);
        pos.push([p.0 - vx, p.1 - vy, 0.0]);
    }
    // the inner outline must still run the same way as the shape
    for i in 0..n {
        let j = (i + 1) % n;
        let (a, b) = (pts[i], pts[j]);
        let (ia, ib) = (pos[2 * i + 1], pos[2 * j + 1]);
        if (ib[0] - ia[0]) * (b.0 - a.0) + (ib[1] - ia[1]) * (b.1 - a.1) <= 0.0 {
            return false;
        }
    }
    let mut idx = Vec::with_capacity(6 * n);
    for i in 0..n as u32 {
        let j = (i + 1) % n as u32;
        let (a, b, c, d) = (2 * i, 2 * i + 1, 2 * j, 2 * j + 1);
        idx.extend_from_slice(&[a, b, c, b, d, c]);
    }
    push_flat(mesh, &pos, &idx, color)
}

pub fn tessellate_path(
    mesh: &mut Mesh,
    path: &Path,
    color: Color,
    mode: TessellationMode,
    stroke_config: &StrokeConfig,
) {
    let mut builder = MeshBuilder::new(mesh, color);
    match mode {
        TessellationMode::Fill => {
            let mut tessellator = FillTessellator::new();
            tessellator
                .tessellate_path(path, &FillOptions::default(), &mut builder)
                .expect("Failed to tessellate fill");
        }
        TessellationMode::Stroke(weight) => {
            let mut tessellator = StrokeTessellator::new();
            let options = StrokeOptions::default()
                .with_line_width(weight)
                .with_line_cap(stroke_config.line_cap.to_lyon())
                .with_line_join(stroke_config.line_join.to_lyon());

            tessellator
                .tessellate_path(path, &options, &mut builder)
                .expect("Failed to tessellate stroke");
        }
    }
}

pub fn empty_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );

    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, Vec::<[f32; 4]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, Vec::<[f32; 3]>::new());
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, Vec::<[f32; 2]>::new());
    mesh.insert_indices(Indices::U32(Vec::new()));

    mesh
}
