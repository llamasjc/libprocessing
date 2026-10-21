// What a material's own vertex shader needs to place a mesh: the camera
// (view) and the mesh's transform (world_from_local). The layouts are
// Bevy's (bevy_render view.wgsl, bevy_pbr mesh_types.wgsl) and have to
// stay in step with the Bevy version libprocessing uses.
//
//   import processing::vertex::{view, world_from_local};
//   @vertex fn vertex(@builtin(instance_index) i: u32, @location(0) p: vec3<f32>) -> ... {
//       let clip = view.clip_from_world * world_from_local(i) * vec4(p, 1.0);

struct View {
    clip_from_world: mat4x4<f32>,
    unjittered_clip_from_world: mat4x4<f32>,
    world_from_clip: mat4x4<f32>,
    world_from_view: mat4x4<f32>,
    view_from_world: mat4x4<f32>,
    clip_from_view: mat4x4<f32>,
    view_from_clip: mat4x4<f32>,
}

@group(0) @binding(0) var<uniform> view: View;

// Bevy's per-mesh data (bevy_pbr mesh_types.wgsl `Mesh`), 176 bytes each,
// read as vec4s since the shader reflection can't take an array of structs:
// the first three are world_from_local, an affine 4x3 transposed to 3x4.
const MESH_VEC4S: u32 = 11u;
@group(2) @binding(0) var<storage> mesh: array<vec4<f32>>;

fn world_from_local(instance_index: u32) -> mat4x4<f32> {
    let at = instance_index * MESH_VEC4S;
    return transpose(mat4x4<f32>(mesh[at], mesh[at + 1u], mesh[at + 2u], vec4<f32>(0.0, 0.0, 0.0, 1.0)));
}
