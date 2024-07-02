mod texture;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use emerald_core::hw::holly::pvr::{
    display_list::{DisplayListBuilder, DisplayListItem, VertexDefinition},
    ta::PvrListType,
    texture_cache::{TextureAtlas, TextureId},
    Pvr,
};
pub use texture::*;
use wgpu::{BindGroup, BindGroupLayout, Color, Device, COPY_BUFFER_ALIGNMENT};

pub const OPAQUE_PASS: usize = 0;
pub const OPAQUE_WF_PASS: usize = 1;
pub const ALPHA_PASS: usize = 2;
pub const ALPHA_WF_PASS: usize = 3;
pub const BLIT_PASS: usize = 4;

#[repr(usize)]
#[derive(Copy, Clone, Debug)]
pub enum HwRasterizerPassKind {
    Opaque = 0,
    Transparent,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct BlitVertex {
    position: [f32; 3],
    uv: [f32; 2],
}

impl BlitVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;

        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct GpuVertex {
    position: [f32; 3],
    color: [f32; 4],
    uv: [f32; 2],

    texture_array_id: u32,
    texture_id: u32,
    textured: u32,
    ignore_alpha: u32,
    shading: u32,
}

impl GpuVertex {
    const ATTRIBS: [wgpu::VertexAttribute; 8] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4, 2 => Float32x2, 3 => Uint32, 4 => Uint32, 5 => Uint32, 6 => Uint32, 7 => Uint32];

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        use std::mem;

        wgpu::VertexBufferLayout {
            array_stride: mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

const PRIMITIVE_RESTART_INDEX: u16 = 0xFFFF;
const BLIT_QUAD_VERTICES: &[BlitVertex] = &[
    BlitVertex {
        position: [-1.0, 1.0, 0.0], // top-left
        uv: [0.0, 0.0],
    },
    BlitVertex {
        position: [1.0, 1.0, 0.0], // top-right
        uv: [1.0, 0.0],
    },
    BlitVertex {
        position: [-1.0, -1.0, 0.0], // bottom-left
        uv: [0.0, 1.0],
    },
    BlitVertex {
        position: [1.0, -1.0, 0.0], // bottom-right
        uv: [1.0, 1.0],
    },
];

const BLIT_QUAD_INDICES: &[u16] = &[0, 2, 1, 1, 2, 3];

pub struct HardwareRasterizer<'a> {
    device: wgpu::Device,
    queue: wgpu::Queue,
    wireframe: bool,

    pipelines: [wgpu::RenderPipeline; 5],

    depth_texture: Texture,
    texture_arrays: [Texture; 8],
    framebuffer_texture: Texture,
    blit_bind_group: BindGroup,
    bind_group: BindGroup,
    blit_bind_group_layout: BindGroupLayout,

    opaque_vertex_buffer: wgpu::Buffer,
    opaque_index_buffer: wgpu::Buffer,

    opaque_verts: Vec<GpuVertex>,
    opaque_indices: Vec<u16>,
    transparent_verts: Vec<GpuVertex>,
    transparent_indices: Vec<u16>,

    transparent_vertex_buffer: wgpu::Buffer,
    transparent_index_buffer: wgpu::Buffer,

    blit_vertex_buffer: wgpu::Buffer,
    blit_index_buffer: wgpu::Buffer,

    pub surface: wgpu::Surface<'a>,

    // maps a texture id to a texture array index in its respective texture array
    // the texture array index is determined by the dimensions of the texture
    gpu_cache_maps: [HashMap<TextureId, usize>; 8],
    texture_free_lists: [TextureFreeList; 8],
}

impl<'a> HardwareRasterizer<'a> {
    pub fn new(window: &sdl2::video::Window) -> Self {
        pollster::block_on(HardwareRasterizer::init(&window))
    }

    pub fn toggle_wireframe(&mut self) {
        self.wireframe = !self.wireframe;
    }

    pub fn blit_fb(&mut self, vram: Vec<u8>, width: u32, height: u32) {
        let output = self.surface.get_current_texture().unwrap();
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        if vram.len() == 0 {
            return;
        }

        self.framebuffer_texture =
            Texture::create_texture(&self.device, width as usize, 480 as usize);

        self.blit_bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &self.blit_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&self.framebuffer_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.framebuffer_texture.sampler),
                },
            ],
            label: Some("blit-bind-group"),
        });

        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.framebuffer_texture.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0 as u32,
                    y: 0 as u32,
                    z: 0 as u32,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &vram,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width: width,
                height: height,
                depth_or_array_layers: 1,
            },
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("blit-encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            render_pass.set_pipeline(&self.pipelines[BLIT_PASS]);
            render_pass.set_bind_group(0, &self.blit_bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.blit_vertex_buffer.slice(..));
            render_pass
                .set_index_buffer(self.blit_index_buffer.slice(..), wgpu::IndexFormat::Uint16);
            render_pass.draw_indexed(0..BLIT_QUAD_INDICES.len() as u32, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }

    pub fn render(
        &mut self,
        display_list_id: u32,
        texture_atlas: Arc<RwLock<TextureAtlas>>,
        vram: Arc<RwLock<Vec<u8>>>,
        pram: Arc<RwLock<Vec<u8>>>,
        bg_verts: [VertexDefinition; 4],
        mut display_lists: [DisplayListBuilder; 5],
    ) {
        let output = self.surface.get_current_texture().unwrap();
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut lists = &mut display_lists;
        self.upload_texture_cache(texture_atlas.clone(), vram.clone(), pram.clone());
        self.build_opaque(
            texture_atlas,
            vram.clone(),
            pram.clone(),
            bg_verts,
            lists,
            display_list_id,
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("opaque-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture.view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            let opaque_pass_idx = if self.wireframe {
                OPAQUE_WF_PASS
            } else {
                OPAQUE_PASS
            };

            render_pass.set_pipeline(&self.pipelines[opaque_pass_idx]);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.opaque_vertex_buffer.slice(..));
            render_pass.set_index_buffer(
                self.opaque_index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );
            render_pass.draw_indexed(0..self.opaque_indices.len() as u32, 0, 0..1);
        }

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("alpha-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                occlusion_query_set: None,
                timestamp_writes: None,
            });

            let alpha_pass_idx = if self.wireframe {
                ALPHA_WF_PASS
            } else {
                ALPHA_PASS
            };

            render_pass.set_pipeline(&self.pipelines[alpha_pass_idx]);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.set_vertex_buffer(0, self.transparent_vertex_buffer.slice(..));
            render_pass.set_index_buffer(
                self.transparent_index_buffer.slice(..),
                wgpu::IndexFormat::Uint16,
            );
            render_pass.draw_indexed(0..self.transparent_indices.len() as u32, 0, 0..1);
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        self.opaque_verts.clear();
        self.opaque_indices.clear();
        self.transparent_verts.clear();
        self.transparent_indices.clear();

        output.present();
    }

    pub fn build_opaque(
        &mut self,
        texture_atlas: Arc<RwLock<TextureAtlas>>,
        vram: Arc<RwLock<Vec<u8>>>,
        pram: Arc<RwLock<Vec<u8>>>,
        bg_verts: [VertexDefinition; 4],
        lists: &mut [DisplayListBuilder],
        display_list_id: u32,
    ) {
        {
            self.build_bg_verts(bg_verts);

            let indices: &[u16] = &[0, 1, 2, 3, PRIMITIVE_RESTART_INDEX];
            self.opaque_indices.extend(indices);

            self.build_opaque_verts(
                texture_atlas.clone(),
                vram.clone(),
                pram.clone(),
                lists,
                display_list_id,
            );
            self.build_transparent_verts(
                texture_atlas.clone(),
                vram.clone(),
                pram.clone(),
                lists,
                display_list_id,
            );
        }

        let data_size = (self.opaque_indices.len() * std::mem::size_of::<u16>()) as u64;
        let padding_size =
            (COPY_BUFFER_ALIGNMENT - (data_size % COPY_BUFFER_ALIGNMENT)) % COPY_BUFFER_ALIGNMENT;

        // Calculate the number of padding elements (u32s) needed
        let padding_elements = padding_size as usize / std::mem::size_of::<u16>();

        self.opaque_indices
            .extend(std::iter::repeat(PRIMITIVE_RESTART_INDEX).take(padding_elements));

        // fixme: move this to its own render method
        self.queue.write_buffer(
            &self.opaque_vertex_buffer,
            0,
            bytemuck::cast_slice(&self.opaque_verts),
        );

        self.queue.write_buffer(
            &self.opaque_index_buffer,
            0,
            bytemuck::cast_slice(&self.opaque_indices),
        );

        let data_size = (self.transparent_indices.len() * std::mem::size_of::<u16>()) as u64;
        let padding_size =
            (COPY_BUFFER_ALIGNMENT - (data_size % COPY_BUFFER_ALIGNMENT)) % COPY_BUFFER_ALIGNMENT;

        let padding_elements = padding_size as usize / std::mem::size_of::<u16>();

        self.transparent_indices
            .extend(std::iter::repeat(PRIMITIVE_RESTART_INDEX).take(padding_elements));

        self.queue.write_buffer(
            &self.transparent_vertex_buffer,
            0,
            bytemuck::cast_slice(&self.transparent_verts),
        );

        self.queue.write_buffer(
            &self.transparent_index_buffer,
            0,
            bytemuck::cast_slice(&self.transparent_indices),
        );
    }

    pub fn texture_indices_from_texture_id(
        &self,
        texture_atlas: Arc<RwLock<TextureAtlas>>,
        vram: Arc<RwLock<Vec<u8>>>,
        pram: Arc<RwLock<Vec<u8>>>,
        texture_id: TextureId,
    ) -> (u32, u32) {
        let width = texture_atlas.read().unwrap().textures[&texture_id].width;
        let texture_array_index: usize = match width {
            8 => 0,
            16 => 1,
            32 => 2,
            64 => 3,
            128 => 4,
            256 => 5,
            512 => 6,
            1024 => 7,
            _ => unreachable!(),
        };

        (
            if self.gpu_cache_maps[texture_array_index]
                .get(&texture_id)
                .is_none()
            {
                0xff
            } else {
                texture_array_index as u32
            },
            *self.gpu_cache_maps[texture_array_index]
                .get(&texture_id)
                .unwrap_or(&0) as u32,
        )
    }

    pub fn build_opaque_verts(
        &mut self,
        texture_atlas: Arc<RwLock<TextureAtlas>>,
        vram: Arc<RwLock<Vec<u8>>>,
        pram: Arc<RwLock<Vec<u8>>>,
        mut lists: &mut [DisplayListBuilder],
        display_list_id: u32,
    ) {
        let mut pdl = &mut lists;
        let mut built_display_list = None;

        let mut opaque_dl = &mut lists[PvrListType::Opaque as usize];

        if let dlb = opaque_dl.build() {
            built_display_list = Some(dlb);
        }

        if let Some(dli) = &built_display_list {
            for item in dli.items.iter() {
                match item {
                    DisplayListItem::UploadTexture {
                        texture,
                        pixel_format,
                        texture_format,
                    } => {
                        texture_atlas.write().unwrap().upload_texture(
                            *texture,
                            &mut vram.write().unwrap(),
                            &mut pram.write().unwrap(),
                            *texture_format,
                            *pixel_format,
                        );
                    }
                    _ => {}
                }
            }

            self.upload_texture_cache(texture_atlas.clone(), vram.clone(), pram.clone());

            for item in dli.items.iter() {
                match item {
                    DisplayListItem::Polygon(poly) => {
                        let s = poly.starting_vert;
                        let num_verts = poly.vert_len;
                        let verts = &dli.verts[s..s + num_verts as usize];

                        self.opaque_verts.extend(
                            verts
                                .iter()
                                .map(|v| GpuVertex {
                                    ignore_alpha: if !poly.tsp.use_alpha() { 1 } else { 0 },
                                    textured: if poly.texture_id.is_some() { 1 } else { 0 },
                                    shading: poly.tsp.texture_shading_instr() as u32,
                                    position: [v.x, v.y, v.z],
                                    texture_array_id: if poly.texture_id.is_none() {
                                        0xff
                                    } else {
                                        self.texture_indices_from_texture_id(
                                            texture_atlas.clone(),
                                            vram.clone(),
                                            pram.clone(),
                                            poly.texture_id.unwrap(),
                                        )
                                        .0
                                    },
                                    texture_id: if poly.texture_id.is_none() {
                                        0xff
                                    } else {
                                        self.texture_indices_from_texture_id(
                                            texture_atlas.clone(),
                                            vram.clone(),
                                            pram.clone(),
                                            poly.texture_id.unwrap(),
                                        )
                                        .1
                                    },
                                    uv: [v.u, v.v],
                                    color: [
                                        ((v.color & 0xFF) as f32) / 255.0,
                                        (((v.color >> 8) & 0xFF) as f32) / 255.0,
                                        (((v.color >> 16) & 0xFF) as f32) / 255.0,
                                        1.0,
                                    ],
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                    _ => {}
                }
            }

            Self::update_indices_for_display_list(
                &self.device,
                &mut self.opaque_indices,
                &dli.items,
                4,
            );
        }
    }

    pub fn build_transparent_verts(
        &mut self,
        texture_atlas: Arc<RwLock<TextureAtlas>>,
        vram: Arc<RwLock<Vec<u8>>>,
        pram: Arc<RwLock<Vec<u8>>>,
        mut lists: &mut [DisplayListBuilder],
        display_list_id: u32,
    ) {
        let mut pdl = &mut lists;
        let mut built_display_list = None;

        let mut opaque_dl = &mut lists[PvrListType::Translucent as usize];

        if let dlb = opaque_dl.build() {
            built_display_list = Some(dlb);
        }

        if let Some(dli) = &built_display_list {
            for item in dli.items.iter() {
                match item {
                    DisplayListItem::UploadTexture {
                        texture,
                        pixel_format,
                        texture_format,
                    } => {
                        texture_atlas.write().unwrap().upload_texture(
                            *texture,
                            &mut vram.write().unwrap(),
                            &mut pram.write().unwrap(),
                            *texture_format,
                            *pixel_format,
                        );
                    }
                    _ => {}
                }
            }

            self.upload_texture_cache(texture_atlas.clone(), vram.clone(), pram.clone());

            for item in dli.items.iter() {
                match item {
                    DisplayListItem::Polygon(poly) => {
                        let s = poly.starting_vert;
                        let num_verts = poly.vert_len;
                        let verts = &dli.verts[s..s + num_verts as usize];

                        self.transparent_verts.extend(
                            verts
                                .iter()
                                .map(|v| GpuVertex {
                                    position: [v.x, v.y, v.z],
                                    ignore_alpha: if !poly.tsp.use_alpha() { 1 } else { 0 },
                                    textured: if poly.texture_id.is_some() { 1 } else { 0 },
                                    shading: poly.tsp.texture_shading_instr() as u32,
                                    texture_array_id: if poly.texture_id.is_none() {
                                        0xff
                                    } else {
                                        self.texture_indices_from_texture_id(
                                            texture_atlas.clone(),
                                            vram.clone(),
                                            pram.clone(),
                                            poly.texture_id.unwrap(),
                                        )
                                        .0
                                    },
                                    texture_id: if poly.texture_id.is_none() {
                                        0xff
                                    } else {
                                        self.texture_indices_from_texture_id(
                                            texture_atlas.clone(),
                                            vram.clone(),
                                            pram.clone(),
                                            poly.texture_id.unwrap(),
                                        )
                                        .1
                                    },
                                    uv: [v.u, v.v],
                                    color: [
                                        ((v.color & 0xFF) as f32) / 255.0,
                                        (((v.color >> 8) & 0xFF) as f32) / 255.0,
                                        (((v.color >> 16) & 0xFF) as f32) / 255.0,
                                        (((v.color >> 24) & 0xFF) as f32) / 255.0,
                                    ],
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
                    _ => {}
                }
            }

            Self::update_indices_for_display_list(
                &self.device,
                &mut self.transparent_indices,
                &dli.items,
                0,
            );
        }
    }

    pub fn build_bg_verts(&mut self, bg_verts: [VertexDefinition; 4]) {
        self.opaque_verts.extend(
            bg_verts
                .iter()
                .map(|v| GpuVertex {
                    // fixme: can I stop hardcoding z here if I disable depth testing?
                    position: [v.x, v.y, 9999.0],
                    texture_array_id: 0xff,
                    texture_id: 0xff,

                    ignore_alpha: 0,
                    textured: 0,
                    shading: 0,
                    uv: [0., 0.], // fixme: can bg verts have UVs?
                    color: [
                        (((v.color >> 16) & 0xFF) as f32) / 255.0,
                        (((v.color >> 8) & 0xFF) as f32) / 255.0,
                        ((v.color & 0xFF) as f32) / 255.0,
                        (((v.color >> 24) & 0xFF) as f32) / 255.0,
                    ],
                })
                .collect::<Vec<_>>(),
        );
    }

    // creates indices for a given display list
    fn update_indices_for_display_list(
        device: &Device,
        indices: &mut Vec<u16>,
        display_list: &[DisplayListItem],
        offset: u16,
    ) {
        for item in display_list {
            if let DisplayListItem::Polygon(polygon) = item {
                let start = polygon.starting_vert as u16;
                let length = polygon.vert_len as u16;

                for i in 0..length {
                    indices.push(start + i + offset); // bg vert offset
                }

                indices.push(PRIMITIVE_RESTART_INDEX);
            }
        }
    }

    pub fn upload_texture_cache(
        &mut self,
        texture_atlas: Arc<RwLock<TextureAtlas>>,
        vram: Arc<RwLock<Vec<u8>>>,
        pram: Arc<RwLock<Vec<u8>>>,
    ) {
        texture_atlas.write().unwrap().increment_generation();

        let gen = texture_atlas.read().unwrap().current_generation;
        let threshold_gen = gen.saturating_sub(10000);
        texture_atlas
            .write()
            .unwrap()
            .textures
            .retain(|_, texture| texture.generation >= threshold_gen);

        for texture in texture_atlas
            .write()
            .unwrap()
            .textures
            .iter_mut()
            .filter(|t| t.1.needs_upload)
        {
            texture.1.needs_upload = false;

            let pow2size = std::cmp::max(texture.1.width, texture.1.height);
            let texture_array_index: usize = match pow2size {
                8 => 0,
                16 => 1,
                32 => 2,
                64 => 3,
                128 => 4,
                256 => 5,
                512 => 6,
                1024 => 7,
                _ => unreachable!(),
            };

            let texture_array = &self.gpu_cache_maps[texture_array_index];
            let texture_id = texture.0;

            if let Some(texture) = texture_array.get(&texture_id) {
                // update the texture
                println!("already has an entry for {:?}", texture_id);
                continue;
            } else {
                let free_idx = self.texture_free_lists[texture_array_index]
                    .find_free()
                    .unwrap();
                self.gpu_cache_maps[texture_array_index].insert(*texture_id, free_idx);
                self.texture_free_lists[texture_array_index].set(free_idx);

                println!(
                    "added an an entry for {:?} @ {} {}",
                    texture_id, texture_array_index, free_idx
                );

                let padded_data = if texture.1.width != texture.1.height {
                    let mut padded_data = vec![0; 4 * pow2size as usize * pow2size as usize];
                    for y in 0..texture.1.height {
                        for x in 0..texture.1.width {
                            let src_idx = 4 * (y * texture.1.width + x);
                            let dst_idx = 4 * (y * texture.1.width + x);
                            padded_data[dst_idx as usize..dst_idx as usize + 4].copy_from_slice(
                                &texture.1.data[src_idx as usize..src_idx as usize + 4],
                            );
                        }
                    }
                    padded_data
                } else {
                    texture.1.data.clone()
                };

                self.queue.write_texture(
                    wgpu::ImageCopyTexture {
                        texture: &self.texture_arrays[texture_array_index].texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: 0 as u32,
                            y: 0 as u32,
                            z: free_idx as u32,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &padded_data,
                    wgpu::ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(4 * pow2size),
                        rows_per_image: Some(pow2size),
                    },
                    wgpu::Extent3d {
                        width: texture.1.width,
                        height: texture.1.height,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }

        for t in texture_atlas
            .read()
            .unwrap()
            .textures
            .iter()
            .filter(|t| t.1.generation < threshold_gen && !t.1.needs_upload)
        {
            let texture_array_index: usize = match t.1.width {
                8 => 0,
                16 => 1,
                32 => 2,
                64 => 3,
                128 => 4,
                256 => 5,
                512 => 6,
                1024 => 7,
                _ => unreachable!(),
            };

            // remove from hash map and mark as free in the free list
            if let Some(idx) = self.gpu_cache_maps[texture_array_index].get(&t.0) {
                println!("removing old entry {:?}", t.0);
                self.texture_free_lists[texture_array_index].unset(*idx);
                self.gpu_cache_maps[texture_array_index].remove(&t.0);
            }
        }
    }

    fn create_blit_pipeline(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("./blit.wgsl").into()),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("blit-layout"),
                bind_group_layouts: &[bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit-pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[BlitVertex::desc()],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: "fs_main",
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            depth_stencil: None,
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: Some(wgpu::IndexFormat::Uint16),
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: true,
                conservative: false,
            },
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        render_pipeline
    }

    fn create_render_pipeline(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        pass_kind: HwRasterizerPassKind,
        bind_group_layout: &wgpu::BindGroupLayout,
        wireframe: bool,
    ) -> wgpu::RenderPipeline {
        // my understanding is that a render pipeline is a collection of draw state
        // we'll need one for opaque and one for transparent
        // opaque should have depth testing + depth write
        // transparent should have depth testing + no depth write
        // transparent should also have blending
        match (pass_kind, wireframe) {
            (HwRasterizerPassKind::Opaque, wf) => {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("opaque-shader"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("./shader.wgsl").into()),
                });

                let render_pipeline_layout =
                    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("opaque-layout"),
                        bind_group_layouts: &[bind_group_layout],
                        push_constant_ranges: &[],
                    });

                let render_pipeline =
                    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("opaque-pipeline"),
                        layout: Some(&render_pipeline_layout),
                        vertex: wgpu::VertexState {
                            module: &shader,
                            entry_point: "vs_main",
                            buffers: &[GpuVertex::desc()],
                            compilation_options: Default::default(),
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &shader,
                            entry_point: "fs_main",
                            compilation_options: Default::default(),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: config.format,
                                blend: Some(wgpu::BlendState::REPLACE),
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                        }),
                        depth_stencil: Some(wgpu::DepthStencilState {
                            format: Texture::DEPTH_FORMAT,
                            depth_write_enabled: true,
                            depth_compare: wgpu::CompareFunction::Less,
                            stencil: wgpu::StencilState::default(),
                            bias: wgpu::DepthBiasState::default(),
                        }),
                        primitive: wgpu::PrimitiveState {
                            topology: wgpu::PrimitiveTopology::TriangleStrip,
                            strip_index_format: Some(wgpu::IndexFormat::Uint16),
                            front_face: wgpu::FrontFace::Cw,
                            cull_mode: None, //Some(wgpu::Face::Back),

                            polygon_mode: if wf {
                                wgpu::PolygonMode::Line
                            } else {
                                wgpu::PolygonMode::Fill
                            },
                            unclipped_depth: true,
                            conservative: false,
                        },
                        multisample: wgpu::MultisampleState {
                            count: 1,
                            mask: !0,
                            alpha_to_coverage_enabled: false,
                        },
                        multiview: None,
                    });

                render_pipeline
            }
            (HwRasterizerPassKind::Transparent, wf) => {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("alpha-shader"),
                    source: wgpu::ShaderSource::Wgsl(include_str!("./shader.wgsl").into()),
                });

                let render_pipeline_layout =
                    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("alpha-layout"),
                        bind_group_layouts: &[bind_group_layout],
                        push_constant_ranges: &[],
                    });

                let render_pipeline =
                    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("alpha-pipeline"),
                        layout: Some(&render_pipeline_layout),
                        vertex: wgpu::VertexState {
                            module: &shader,
                            entry_point: "vs_main",
                            buffers: &[GpuVertex::desc()],
                            compilation_options: Default::default(),
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &shader,
                            entry_point: "fs_main",
                            compilation_options: Default::default(),
                            targets: &[Some(wgpu::ColorTargetState {
                                format: config.format,
                                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                        }),
                        depth_stencil: None,
                        primitive: wgpu::PrimitiveState {
                            topology: wgpu::PrimitiveTopology::TriangleStrip,
                            strip_index_format: Some(wgpu::IndexFormat::Uint16),
                            front_face: wgpu::FrontFace::Cw,
                            cull_mode: None, //Some(wgpu::Face::Back),

                            polygon_mode: if wf {
                                wgpu::PolygonMode::Line
                            } else {
                                wgpu::PolygonMode::Fill
                            },
                            unclipped_depth: true,
                            conservative: false,
                        },
                        multisample: wgpu::MultisampleState {
                            count: 1,
                            mask: !0,
                            alpha_to_coverage_enabled: false,
                        },
                        multiview: None,
                    });

                render_pipeline
            }
        }
    }

    async fn init(window: &sdl2::video::Window) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::METAL,
            ..Default::default()
        });

        let surface = unsafe {
            instance
                .create_surface_unsafe(
                    wgpu::SurfaceTargetUnsafe::from_window(&window)
                        .expect("failed to create surface"),
                )
                .expect("failed to create surface")
        };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::POLYGON_MODE_LINE
                        | wgpu::Features::DEPTH_CLIP_CONTROL,
                    required_limits: wgpu::Limits::default(),
                    label: None,
                },
                None,
            )
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    required_features: wgpu::Features::POLYGON_MODE_LINE
                        | wgpu::Features::DEPTH_CLIP_CONTROL,
                    required_limits: wgpu::Limits::default(),
                    label: None,
                },
                None,
            )
            .await
            .unwrap();

        let size = window.size();
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.0,
            height: size.1,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        let depth_texture = Texture::create_depth_texture(&device, &config, "depth_texture");

        // create a vb of a fixed length but without any initial contents
        let opaque_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("opaque-vertex-buffer"),
            size: 268435456,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let opaque_index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("opaque-index-buffer"),
            size: 268435456,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let transparent_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("transparent-vertex-buffer"),
            size: 268435456,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let transparent_index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("transparent-index-buffer"),
            size: 268435456,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let blit_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit-vertex-buffer"),
            size: 268435456,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let blit_index_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("blit-index-buffer"),
            size: 268435456,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let opaque_verts = vec![];
        let opaque_indices = vec![];
        let transparent_verts = vec![];
        let transparent_indices = vec![];
        let wireframe = false;

        let texture_arrays = Texture::create_texture_array(&device);
        let framebuffer_texture = Texture::create_texture(&device, 640, 480);

        let gpu_cache_maps: [HashMap<TextureId, usize>; 8] = Default::default();
        let texture_free_lists: [TextureFreeList; 8] = Default::default();

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
            label: Some("bind-group-layout"),
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_arrays[0].view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture_arrays[1].view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&texture_arrays[2].view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&texture_arrays[3].view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&texture_arrays[4].view),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&texture_arrays[5].view),
                },
                wgpu::BindGroupEntry {
                    binding: 6,
                    resource: wgpu::BindingResource::TextureView(&texture_arrays[6].view),
                },
                wgpu::BindGroupEntry {
                    binding: 7,
                    resource: wgpu::BindingResource::TextureView(&texture_arrays[7].view),
                },
                wgpu::BindGroupEntry {
                    binding: 8,
                    resource: wgpu::BindingResource::Sampler(&texture_arrays[0].sampler),
                },
            ],
            label: Some("bind-group"),
        });

        let blit_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            multisampled: false,
                            view_dimension: wgpu::TextureViewDimension::D2,
                            sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
                label: Some("blit-bind-group-layout"),
            });

        let blit_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            layout: &blit_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&framebuffer_texture.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&framebuffer_texture.sampler),
                },
            ],
            label: Some("blit-bind-group"),
        });

        // fixme: other list types like punch through and mod vols?
        let pipelines = [
            Self::create_render_pipeline(
                &device,
                &config,
                HwRasterizerPassKind::Opaque,
                &bind_group_layout,
                false,
            ),
            Self::create_render_pipeline(
                &device,
                &config,
                HwRasterizerPassKind::Opaque,
                &bind_group_layout,
                true,
            ),
            Self::create_render_pipeline(
                &device,
                &config,
                HwRasterizerPassKind::Transparent,
                &bind_group_layout,
                false,
            ),
            Self::create_render_pipeline(
                &device,
                &config,
                HwRasterizerPassKind::Transparent,
                &bind_group_layout,
                true,
            ),
            Self::create_blit_pipeline(&device, &config, &blit_bind_group_layout),
        ];

        queue.write_buffer(
            &blit_vertex_buffer,
            0,
            bytemuck::cast_slice(&BLIT_QUAD_VERTICES),
        );

        queue.write_buffer(
            &blit_index_buffer,
            0,
            bytemuck::cast_slice(&BLIT_QUAD_INDICES),
        );

        Self {
            device,
            queue,
            gpu_cache_maps,
            texture_free_lists,
            pipelines,
            depth_texture,
            opaque_vertex_buffer,
            opaque_index_buffer,
            transparent_vertex_buffer,
            transparent_index_buffer,
            opaque_verts,
            opaque_indices,
            transparent_verts,
            transparent_indices,
            surface,
            wireframe,
            texture_arrays,
            bind_group,
            blit_bind_group,
            framebuffer_texture,
            blit_index_buffer,
            blit_vertex_buffer,
            blit_bind_group_layout,
        }
    }
}
