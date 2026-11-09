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
