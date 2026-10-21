use bevy::{
    mesh::{Indices, VertexAttributeValues},
    prelude::*,
};
use lyon::{geom::Point, path::Path};

use crate::render::command::ShapeKind;
use crate::render::primitive::{StrokeConfig, TessellationMode, tessellate_path};

#[derive(Debug, Clone)]
pub enum VertexType {
    Normal(f32, f32),
    CubicBezier {
        cx1: f32,
        cy1: f32,
        cx2: f32,
        cy2: f32,
        x: f32,
        y: f32,
    },
    QuadraticBezier {
        cx: f32,
        cy: f32,
        x: f32,
        y: f32,
    },
    CurveVertex(f32, f32),
}

#[derive(Debug, Clone)]
pub struct Contour {
    pub vertices: Vec<VertexType>,
}

#[derive(Debug, Clone)]
pub struct ShapeBuilder {
    pub kind: ShapeKind,
    pub contours: Vec<Contour>,
    pub in_contour: bool,
    /// Fork: full vertices (x, y, z, u, v, has_uv) for 3D and textured shapes.
    pub full: Vec<[f32; 6]>,
}

impl ShapeBuilder {
    /// Fork: a vertex with z and texture coordinates; also recorded as a 2D vertex for the existing builders.
    pub fn push_full(&mut self, x: f32, y: f32, z: f32, u: f32, v: f32, has_uv: bool) {
        self.push_vertex(VertexType::Normal(x, y));
        self.full.push([x, y, z, u, v, if has_uv { 1.0 } else { 0.0 }]);
    }

    pub fn new(kind: ShapeKind) -> Self {
        Self {
            kind,
            contours: vec![Contour {
                vertices: Vec::new(),
            }],
            in_contour: false,
            full: Vec::new(),
        }
    }

    pub fn push_vertex(&mut self, vt: VertexType) {
        let contour = if self.in_contour {
            self.contours.last_mut().unwrap()
        } else {
            &mut self.contours[0]
        };
        contour.vertices.push(vt);
    }

    pub fn begin_contour(&mut self) {
        self.in_contour = true;
        self.contours.push(Contour {
            vertices: Vec::new(),
        });
    }

    pub fn end_contour(&mut self) {
        self.in_contour = false;
    }
}

pub fn build_polygon_fill(
    mesh: &mut Mesh,
    builder: &ShapeBuilder,
    close: bool,
    color: Color,
    stroke_config: &StrokeConfig,
) {
    let path = build_polygon_path(builder, close);
    tessellate_path(mesh, &path, color, TessellationMode::Fill, stroke_config);
}

pub fn build_polygon_stroke(
    mesh: &mut Mesh,
    builder: &ShapeBuilder,
    close: bool,
    color: Color,
    weight: f32,
    stroke_config: &StrokeConfig,
) {
    let path = build_polygon_path(builder, close);
    tessellate_path(
        mesh,
        &path,
        color,
        TessellationMode::Stroke(weight),
        stroke_config,
    );
}

fn build_polygon_path(builder: &ShapeBuilder, close: bool) -> Path {
    let mut pb = Path::builder();

    for (contour_idx, contour) in builder.contours.iter().enumerate() {
        if contour.vertices.is_empty() {
            continue;
        }

        // collect curve vertices and convert to bezier segments
        let expanded = expand_curve_vertices(&contour.vertices);

        let mut started = false;

        for vt in &expanded {
            match vt {
                VertexType::Normal(x, y) => {
                    if !started {
                        pb.begin(Point::new(*x, *y));
                        started = true;
                    } else {
                        pb.line_to(Point::new(*x, *y));
                    }
                }
                VertexType::CubicBezier {
                    cx1,
                    cy1,
                    cx2,
                    cy2,
                    x,
                    y,
                } => {
                    if started {
                        pb.cubic_bezier_to(
                            Point::new(*cx1, *cy1),
                            Point::new(*cx2, *cy2),
                            Point::new(*x, *y),
                        );
                    }
                }
                VertexType::QuadraticBezier { cx, cy, x, y } => {
                    if started {
                        pb.quadratic_bezier_to(Point::new(*cx, *cy), Point::new(*x, *y));
                    }
                }
                VertexType::CurveVertex(..) => {
                    // should have been expanded away
                }
            }
        }

        if started {
            // contours (holes) always close; outer shape closes if requested
            pb.end(close || contour_idx > 0);
        }
    }

    pb.build()
}

/// expand curveVertex entries into cubic bezier segments using catmull-rom to Bezier conversion.
/// non-curve vertices are passed through unchanged.
fn expand_curve_vertices(vertices: &[VertexType]) -> Vec<VertexType> {
    // If no curve vertices, return as-is
    if !vertices
        .iter()
        .any(|v| matches!(v, VertexType::CurveVertex(..)))
    {
        return vertices.to_vec();
    }

    let mut result = Vec::new();
    let mut curve_points: Vec<(f32, f32)> = Vec::new();

    for vt in vertices {
        match vt {
            VertexType::CurveVertex(x, y) => {
                curve_points.push((*x, *y));
            }
            _ => {
                if !curve_points.is_empty() {
                    flush_curve_points(&curve_points, &mut result);
                    curve_points.clear();
                }
                result.push(vt.clone());
            }
        }
    }

    if !curve_points.is_empty() {
        flush_curve_points(&curve_points, &mut result);
    }

    result
}

// convert a sequence of Catmull-Rom curve points into bezier segments.
fn flush_curve_points(points: &[(f32, f32)], result: &mut Vec<VertexType>) {
    if points.len() < 4 {
        // not enough for a curve segment, emit as normal vertices
        for &(x, y) in &points[1..points.len().saturating_sub(1).max(1)] {
            result.push(VertexType::Normal(x, y));
        }
        return;
    }

    // first drawn point is points[1]
    result.push(VertexType::Normal(points[1].0, points[1].1));

    for i in 0..points.len() - 3 {
        let p0 = points[i];
        let p1 = points[i + 1];
        let p2 = points[i + 2];
        let p3 = points[i + 3];

        // catmull-Rom to cubic bezier conversion
        // see https://en.wikipedia.org/wiki/Catmull%E2%80%93Rom_spline#Converting_to_B%C3%A9zier_curve
        let cp1x = p1.0 + (p2.0 - p0.0) / 6.0;
        let cp1y = p1.1 + (p2.1 - p0.1) / 6.0;
        let cp2x = p2.0 - (p3.0 - p1.0) / 6.0;
        let cp2y = p2.1 - (p3.1 - p1.1) / 6.0;

        result.push(VertexType::CubicBezier {
            cx1: cp1x,
            cy1: cp1y,
            cx2: cp2x,
            cy2: cp2y,
            x: p2.0,
            y: p2.1,
        });
    }
}

/// Build direct mesh geometry for non-polygon shape kinds
/// Fork: fill for shapes whose vertices have a z or texture coordinates.
pub fn build_full_fill(mesh: &mut Mesh, builder: &ShapeBuilder, color: Color) {
    let v = &builder.full;
    let n = v.len();
    let mut tri = |a: usize, b: usize, c: usize| push_triangle_full(mesh, color, v[a], v[b], v[c]);
    match builder.kind {
        ShapeKind::Triangles => {
            for i in (0..n.saturating_sub(2)).step_by(3) { tri(i, i + 1, i + 2); }
        }
        ShapeKind::TriangleStrip => {
            for i in 0..n.saturating_sub(2) {
                if i % 2 == 0 { tri(i, i + 1, i + 2) } else { tri(i + 1, i, i + 2) }
            }
        }
        ShapeKind::Quads => {
            for i in (0..n.saturating_sub(3)).step_by(4) { tri(i, i + 1, i + 2); tri(i, i + 2, i + 3); }
        }
        ShapeKind::QuadStrip => {
            let mut i = 0;
            while i + 3 < n { tri(i, i + 1, i + 3); tri(i, i + 3, i + 2); i += 2; }
        }
        ShapeKind::Points | ShapeKind::Lines => {}
        _ => {
            for i in 1..n.saturating_sub(1) { tri(0, i, i + 1); }
        }
    }
}

fn push_triangle_full(mesh: &mut Mesh, color: Color, a: [f32; 6], b: [f32; 6], c: [f32; 6]) {
    let base_idx = if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    {
        positions.len() as u32
    } else {
        0
    };
    if let Some(VertexAttributeValues::Float32x3(positions)) = mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION) {
        positions.push([a[0], a[1], a[2]]);
        positions.push([b[0], b[1], b[2]]);
        positions.push([c[0], c[1], c[2]]);
    }
    let color_array = color.to_linear().to_f32_array();
    if let Some(VertexAttributeValues::Float32x4(colors)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
        for _ in 0..3 { colors.push(color_array); }
    }
    let (ux, uy, uz) = (b[0] - a[0], b[1] - a[1], b[2] - a[2]);
    let (wx, wy, wz) = (c[0] - a[0], c[1] - a[1], c[2] - a[2]);
    let (nx, ny, nz) = (uy * wz - uz * wy, uz * wx - ux * wz, ux * wy - uy * wx);
    let len = (nx * nx + ny * ny + nz * nz).sqrt().max(1e-12);
    let normal = [nx / len, ny / len, nz / len];
    if let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL) {
        for _ in 0..3 { normals.push(normal); }
    }
    if let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        uvs.push([a[3], a[4]]);
        uvs.push([b[3], b[4]]);
        uvs.push([c[3], c[4]]);
    }
    if let Some(indices) = mesh.indices_mut() {
        match indices {
            Indices::U32(ix) => ix.extend_from_slice(&[base_idx, base_idx + 1, base_idx + 2]),
            Indices::U16(ix) => ix.extend_from_slice(&[base_idx as u16, base_idx as u16 + 1, base_idx as u16 + 2]),
        }
    }
}

pub fn build_direct_fill(mesh: &mut Mesh, builder: &ShapeBuilder, color: Color) {
    let vertices: Vec<(f32, f32)> = builder.contours[0]
        .vertices
        .iter()
        .filter_map(|v| match v {
            VertexType::Normal(x, y) => Some((*x, *y)),
            _ => None,
        })
        .collect();

    match builder.kind {
        ShapeKind::Triangles => {
            for chunk in vertices.as_chunks::<3>().0 {
                push_triangle(
                    mesh, color, chunk[0].0, chunk[0].1, chunk[1].0, chunk[1].1, chunk[2].0,
                    chunk[2].1,
                );
            }
        }
        ShapeKind::TriangleFan if vertices.len() >= 3 => {
            let hub = vertices[0];
            for i in 1..vertices.len() - 1 {
                push_triangle(
                    mesh,
                    color,
                    hub.0,
                    hub.1,
                    vertices[i].0,
                    vertices[i].1,
                    vertices[i + 1].0,
                    vertices[i + 1].1,
                );
            }
        }
        ShapeKind::TriangleStrip => {
            for i in 0..vertices.len().saturating_sub(2) {
                if i % 2 == 0 {
                    push_triangle(
                        mesh,
                        color,
                        vertices[i].0,
                        vertices[i].1,
                        vertices[i + 1].0,
                        vertices[i + 1].1,
                        vertices[i + 2].0,
                        vertices[i + 2].1,
                    );
                } else {
                    // Reverse winding for odd triangles
                    push_triangle(
                        mesh,
                        color,
                        vertices[i + 1].0,
                        vertices[i + 1].1,
                        vertices[i].0,
                        vertices[i].1,
                        vertices[i + 2].0,
                        vertices[i + 2].1,
                    );
                }
            }
        }
        ShapeKind::Quads => {
            for chunk in vertices.as_chunks::<4>().0 {
                push_quad(
                    mesh, color, chunk[0].0, chunk[0].1, chunk[1].0, chunk[1].1, chunk[2].0,
                    chunk[2].1, chunk[3].0, chunk[3].1,
                );
            }
        }
        ShapeKind::QuadStrip => {
            let mut i = 0;
            while i + 3 < vertices.len() {
                push_quad(
                    mesh,
                    color,
                    vertices[i].0,
                    vertices[i].1,
                    vertices[i + 1].0,
                    vertices[i + 1].1,
                    vertices[i + 3].0,
                    vertices[i + 3].1,
                    vertices[i + 2].0,
                    vertices[i + 2].1,
                );
                i += 2;
            }
        }
        _ => {}
    }
}

/// Build stroke outlines for non-polygon shape kinds.
pub fn build_direct_stroke(
    mesh: &mut Mesh,
    builder: &ShapeBuilder,
    color: Color,
    weight: f32,
    stroke_config: &StrokeConfig,
) {
    let vertices: Vec<(f32, f32)> = builder.contours[0]
        .vertices
        .iter()
        .filter_map(|v| match v {
            VertexType::Normal(x, y) => Some((*x, *y)),
            _ => None,
        })
        .collect();

    match builder.kind {
        ShapeKind::Lines => {
            for chunk in vertices.as_chunks::<2>().0 {
                let mut pb = Path::builder();
                pb.begin(Point::new(chunk[0].0, chunk[0].1));
                pb.line_to(Point::new(chunk[1].0, chunk[1].1));
                pb.end(false);
                tessellate_path(
                    mesh,
                    &pb.build(),
                    color,
                    TessellationMode::Stroke(weight),
                    stroke_config,
                );
            }
        }
        ShapeKind::Triangles => {
            for chunk in vertices.as_chunks::<3>().0 {
                stroke_polygon(
                    mesh,
                    &[chunk[0], chunk[1], chunk[2]],
                    true,
                    color,
                    weight,
                    stroke_config,
                );
            }
        }
        ShapeKind::TriangleFan if vertices.len() >= 3 => {
            let hub = vertices[0];
            for i in 1..vertices.len() - 1 {
                stroke_polygon(
                    mesh,
                    &[hub, vertices[i], vertices[i + 1]],
                    true,
                    color,
                    weight,
                    stroke_config,
                );
            }
        }
        ShapeKind::TriangleStrip => {
            for i in 0..vertices.len().saturating_sub(2) {
                if i % 2 == 0 {
                    stroke_polygon(
                        mesh,
                        &[vertices[i], vertices[i + 1], vertices[i + 2]],
                        true,
                        color,
                        weight,
                        stroke_config,
                    );
                } else {
                    stroke_polygon(
                        mesh,
                        &[vertices[i + 1], vertices[i], vertices[i + 2]],
                        true,
                        color,
                        weight,
                        stroke_config,
                    );
                }
            }
        }
        ShapeKind::Quads => {
            for chunk in vertices.as_chunks::<4>().0 {
                stroke_polygon(
                    mesh,
                    &[chunk[0], chunk[1], chunk[2], chunk[3]],
                    true,
                    color,
                    weight,
                    stroke_config,
                );
            }
        }
        ShapeKind::QuadStrip => {
            let mut i = 0;
            while i + 3 < vertices.len() {
                stroke_polygon(
                    mesh,
                    &[
                        vertices[i],
                        vertices[i + 1],
                        vertices[i + 3],
                        vertices[i + 2],
                    ],
                    true,
                    color,
                    weight,
                    stroke_config,
                );
                i += 2;
            }
        }
        _ => {}
    }
}

fn stroke_polygon(
    mesh: &mut Mesh,
    verts: &[(f32, f32)],
    close: bool,
    color: Color,
    weight: f32,
    stroke_config: &StrokeConfig,
) {
    if verts.is_empty() {
        return;
    }
    let mut pb = Path::builder();
    pb.begin(Point::new(verts[0].0, verts[0].1));
    for v in &verts[1..] {
        pb.line_to(Point::new(v.0, v.1));
    }
    pb.end(close);
    tessellate_path(
        mesh,
        &pb.build(),
        color,
        TessellationMode::Stroke(weight),
        stroke_config,
    );
}

fn push_triangle(
    mesh: &mut Mesh,
    color: Color,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    x3: f32,
    y3: f32,
) {
    let base_idx = if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    {
        positions.len() as u32
    } else {
        0
    };

    if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        positions.push([x1, y1, 0.0]);
        positions.push([x2, y2, 0.0]);
        positions.push([x3, y3, 0.0]);
    }

    let color_array = color.to_linear().to_f32_array();
    if let Some(VertexAttributeValues::Float32x4(colors)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
    {
        for _ in 0..3 {
            colors.push(color_array);
        }
    }

    if let Some(VertexAttributeValues::Float32x3(normals)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    {
        for _ in 0..3 {
            normals.push([0.0, 0.0, 1.0]);
        }
    }

    if let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        uvs.push([0.0, 0.0]);
        uvs.push([1.0, 0.0]);
        uvs.push([0.5, 1.0]);
    }

    if let Some(Indices::U32(indices)) = mesh.indices_mut() {
        indices.push(base_idx);
        indices.push(base_idx + 1);
        indices.push(base_idx + 2);
    }
}

fn push_quad(
    mesh: &mut Mesh,
    color: Color,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    x3: f32,
    y3: f32,
    x4: f32,
    y4: f32,
) {
    let base_idx = if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute(Mesh::ATTRIBUTE_POSITION)
    {
        positions.len() as u32
    } else {
        0
    };

    if let Some(VertexAttributeValues::Float32x3(positions)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
    {
        positions.push([x1, y1, 0.0]);
        positions.push([x2, y2, 0.0]);
        positions.push([x3, y3, 0.0]);
        positions.push([x4, y4, 0.0]);
    }

    let color_array = color.to_linear().to_f32_array();
    if let Some(VertexAttributeValues::Float32x4(colors)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR)
    {
        for _ in 0..4 {
            colors.push(color_array);
        }
    }

    if let Some(VertexAttributeValues::Float32x3(normals)) =
        mesh.attribute_mut(Mesh::ATTRIBUTE_NORMAL)
    {
        for _ in 0..4 {
            normals.push([0.0, 0.0, 1.0]);
        }
    }

    if let Some(VertexAttributeValues::Float32x2(uvs)) = mesh.attribute_mut(Mesh::ATTRIBUTE_UV_0) {
        uvs.push([0.0, 0.0]);
        uvs.push([1.0, 0.0]);
        uvs.push([1.0, 1.0]);
        uvs.push([0.0, 1.0]);
    }

    if let Some(Indices::U32(indices)) = mesh.indices_mut() {
        indices.push(base_idx);
        indices.push(base_idx + 1);
        indices.push(base_idx + 2);

        indices.push(base_idx);
        indices.push(base_idx + 2);
        indices.push(base_idx + 3);
    }
}
