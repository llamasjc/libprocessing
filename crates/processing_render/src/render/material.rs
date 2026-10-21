use bevy::math::Affine2;
use bevy::pbr::ExtendedMaterial;
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use std::ops::Deref;

use crate::material::ProcessingMaterial;
use crate::material::custom::{CustomMaterial, CustomMaterial3d};

#[derive(Component, Deref)]
pub struct UntypedMaterial(pub UntypedHandle);

pub type ProcessingExtendedMaterial = ExtendedMaterial<StandardMaterial, ProcessingMaterial>;

#[derive(Clone, PartialEq, Debug)]
pub enum MaterialKey {
    Color {
        transparent: bool,
        background_image: Option<Handle<Image>>,
        uv_transform: Affine2,
        blend_state: Option<BlendState>,
    },
    Pbr {
        albedo: [u8; 4],
        roughness: u8,
        metallic: u8,
        emissive: [u8; 4],
        base_color_texture: Option<Handle<Image>>,
        uv_transform: Affine2,
        blend_state: Option<BlendState>,
    },
    Custom {
        entity: Entity,
        blend_state: Option<BlendState>,
    },
}

pub struct PbrFields {
    pub albedo: [u8; 4],
    pub roughness: u8,
    pub metallic: u8,
    pub emissive: [u8; 4],
    pub base_color_texture: Option<Handle<Image>>,
    pub uv_transform: Affine2,
    pub blend_state: Option<BlendState>,
}

impl Default for PbrFields {
    fn default() -> Self {
        Self {
            albedo: [255, 255, 255, 255],
            roughness: 128,
            metallic: 0,
            emissive: [0, 0, 0, 0],
            base_color_texture: None,
            uv_transform: Affine2::IDENTITY,
            blend_state: None,
        }
    }
}

impl From<PbrFields> for MaterialKey {
    fn from(f: PbrFields) -> Self {
        MaterialKey::Pbr {
            albedo: f.albedo,
            roughness: f.roughness,
            metallic: f.metallic,
            emissive: f.emissive,
            base_color_texture: f.base_color_texture,
            uv_transform: f.uv_transform,
            blend_state: f.blend_state,
        }
    }
}

impl MaterialKey {
    pub fn as_pbr(&self) -> PbrFields {
        match self {
            MaterialKey::Pbr {
                albedo,
                roughness,
                metallic,
                emissive,
                base_color_texture,
                uv_transform,
                blend_state,
            } => PbrFields {
                albedo: *albedo,
                roughness: *roughness,
                metallic: *metallic,
                emissive: *emissive,
                base_color_texture: base_color_texture.clone(),
                uv_transform: *uv_transform,
                blend_state: *blend_state,
            },
            _ => PbrFields::default(),
        }
    }

    pub fn blend_state(&self) -> Option<BlendState> {
        match self {
            MaterialKey::Color { blend_state, .. } => *blend_state,
            MaterialKey::Pbr { blend_state, .. } => *blend_state,
            MaterialKey::Custom { blend_state, .. } => *blend_state,
        }
    }

    fn to_standard_material(&self) -> StandardMaterial {
        match self {
            MaterialKey::Color {
                transparent,
                background_image,
                uv_transform,
                blend_state,
            } => StandardMaterial {
                base_color: Color::WHITE,
                unlit: true,
                cull_mode: None,
                base_color_texture: background_image.clone(),
                uv_transform: *uv_transform,
                alpha_mode: if blend_state.is_some() || *transparent {
                    AlphaMode::Blend
                } else if background_image.is_some() {
                    // Fork: images blend with their own alpha (soft edges); the depth bias keeps the order
                    AlphaMode::Blend
                } else {
                    AlphaMode::Opaque
                },
                ..default()
            },
            MaterialKey::Pbr {
                albedo,
                roughness,
                metallic,
                emissive,
                base_color_texture,
                uv_transform,
                ..
            } => {
                let base_color = Color::srgba(
                    albedo[0] as f32 / 255.0,
                    albedo[1] as f32 / 255.0,
                    albedo[2] as f32 / 255.0,
                    albedo[3] as f32 / 255.0,
                );
                StandardMaterial {
                    base_color,
                    unlit: false,
                    cull_mode: None,
                    perceptual_roughness: *roughness as f32 / 255.0,
                    metallic: *metallic as f32 / 255.0,
                    emissive: LinearRgba::new(
                        emissive[0] as f32 / 255.0,
                        emissive[1] as f32 / 255.0,
                        emissive[2] as f32 / 255.0,
                        emissive[3] as f32 / 255.0,
                    ),
                    base_color_texture: base_color_texture.clone(),
                    uv_transform: *uv_transform,
                    ..default()
                }
            }
            MaterialKey::Custom { .. } => unreachable!(),
        }
    }

    pub fn to_material(
        &self,
        materials: &mut ResMut<Assets<ProcessingExtendedMaterial>>,
    ) -> UntypedHandle {
        self.to_material_biased(materials, 0.0)
    }

    /// CppMode: blended meshes are sorted by distance, and later draws sit further
    /// back, so without a bias they stack in reverse. depth_bias is added to the
    /// sort distance; spawn_mesh passes one that grows with the draw index.
    pub fn to_material_biased(
        &self,
        materials: &mut ResMut<Assets<ProcessingExtendedMaterial>>,
        depth_bias: f32,
    ) -> UntypedHandle {
        self.to_material_clipped(materials, depth_bias, None)
    }

    /// cppmode-clip: same as to_material_biased, plus the clip() rect the shader discards outside of
    pub fn to_material_clipped(
        &self,
        materials: &mut ResMut<Assets<ProcessingExtendedMaterial>>,
        depth_bias: f32,
        clip_rect: Option<[f32; 4]>,
    ) -> UntypedHandle {
        let blend_state = self.blend_state();
        let mut base = self.to_standard_material();
        if matches!(base.alpha_mode, AlphaMode::Blend) {
            base.depth_bias = depth_bias;
        }
        let extended = ProcessingExtendedMaterial {
            base,
            extension: ProcessingMaterial {
                blend_state,
                depth_write: None,
                clip_rect: clip_vec(clip_rect),
            },
        };
        materials.add(extended).untyped()
    }
}

pub fn add_processing_materials(mut commands: Commands, meshes: Query<(Entity, &UntypedMaterial)>) {
    for (entity, handle) in meshes.iter() {
        let handle = handle.deref().clone();
        if let Ok(handle) = handle.try_typed::<ProcessingExtendedMaterial>() {
            commands.entity(entity).insert(MeshMaterial3d(handle));
        }
    }
}

pub fn add_custom_materials(mut commands: Commands, meshes: Query<(Entity, &UntypedMaterial)>) {
    for (entity, handle) in meshes.iter() {
        let handle = handle.deref().clone();
        if let Ok(handle) = handle.try_typed::<CustomMaterial>() {
            commands.entity(entity).insert(CustomMaterial3d(handle));
        }
    }
}

/// cppmode-clip: clip rect for the shader; zero width means "no clip"
pub fn clip_vec(rect: Option<[f32; 4]>) -> bevy::math::Vec4 {
    match rect {
        Some([x, y, w, h]) if w > 0.0 && h > 0.0 => bevy::math::Vec4::new(x, y, w, h),
        Some(_) => bevy::math::Vec4::new(0.0, 0.0, 1e-6, 1e-6), // empty clip: hide everything
        None => bevy::math::Vec4::ZERO,
    }
}
