use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{Arc, RwLock},
};

use rect_packer::Packer;
use serde::{Deserialize, Serialize};

use crate::hw::sh4::bus::PhysicalAddress;

use super::ta::{PvrPixelFmt, PvrTextureFmt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextureId(pub u64);

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub addr: u32,
    pub palette_addr: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextureMetadata {
    pub id: TextureId,
    pub width: u32,
    pub height: u32,
    pub generation: u64,
    pub texture_format: PvrTextureFmt,
    pub pixel_format: PvrPixelFmt,
    pub palette_addr: u32,

    #[serde(skip)]
    pub data: Vec<u8>,

    pub dirty: bool,
    pub needs_upload: bool,
    pub addr: u32,
    pub end_addr: u32,
}

impl TextureMetadata {
    pub fn texture_size(
        width: u32,
        height: u32,
        pixel_format: PvrPixelFmt,
        texture_format: PvrTextureFmt,
    ) -> usize {
        let bits_per_pixel = match pixel_format {
            PvrPixelFmt::EightBpp => 8,
            PvrPixelFmt::FourBpp => 4,
            _ if texture_format == PvrTextureFmt::Vq
                || texture_format == PvrTextureFmt::VqMipmaps =>
            {
                16
            }
            _ => 128,
        };

        let bytes_per_pixel = bits_per_pixel / 8;
        (width * height * bytes_per_pixel) as usize
    }
}

#[derive(Copy, Clone, Debug)]
pub struct TextureEntry {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TextureAtlas {
    width: u32,
    height: u32,
    #[serde(skip)]
    pub data: Vec<u8>,
    pub textures: HashMap<TextureId, TextureMetadata>,
    pub current_generation: u64,
}

impl TextureAtlas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; (width * height * 4) as usize],
            textures: HashMap::new(),
            current_generation: 0,
        }
    }

    pub fn hash_texture(
        texture: &Texture,
        texture_format: PvrTextureFmt,
        pixel_format: PvrPixelFmt,
    ) -> u64 {
        let mut hasher = DefaultHasher::new();

        texture.width.hash(&mut hasher);
        texture.height.hash(&mut hasher);
        texture.addr.hash(&mut hasher);
        texture_format.hash(&mut hasher);
        pixel_format.hash(&mut hasher);
        texture.palette_addr.hash(&mut hasher);
        hasher.finish()
    }

    fn morton_order(x: u32, y: u32) -> u32 {
        let mut morton = 0;
        for i in 0..16 {
            morton |= ((x >> i) & 1) << (2 * i);
            morton |= ((y >> i) & 1) << (2 * i + 1);
        }
        morton
    }

    fn convert_twiddled_rgb4444_to_argb8888(data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let mut abgr_data: Vec<u8> = Vec::with_capacity((width * height * 4) as usize);
        abgr_data.resize((width * height * 4) as usize, 0);

        for y in 0..height {
            for x in 0..width {
                let morton_index = Self::morton_order(x, y) as usize;
                let chunk_index = morton_index * 2;

                let chunk = &data[chunk_index..chunk_index + 2];
                let pixel = u16::from_le_bytes([chunk[0], chunk[1]]);

                let a = ((pixel >> 12) & 0xF) as u8; // Extract and scale alpha
                let r = ((pixel >> 8) & 0xF) as u8; // Extract and scale red
                let g = ((pixel >> 4) & 0xF) as u8; // Extract and scale green
                let b = (pixel & 0xF) as u8; // Extract and scale blue

                let linear_index = ((y * width + x) * 4) as usize;

                // Store in ABGR order matching the SDL PixelFormatEnum::ABGR8888
                abgr_data[linear_index] = (b << 4) | b as u8;
                abgr_data[linear_index + 1] = (g << 4) | g as u8;
                abgr_data[linear_index + 2] = (r << 4) | r as u8;
                abgr_data[linear_index + 3] = (a << 4) | a as u8;
            }
        }
        abgr_data
    }

    fn convert_twiddled_8bpp_to_rgba8888(
        data: &[u8],
        pram: &[u8],
        palette_selector: u32,
        width: u32,
        height: u32,
    ) -> Vec<u8> {
        let mut rgba_data = vec![0; (width * height * 4) as usize];
        let palette_selector = (palette_selector & 0x03) as u16;

        for y in 0..height {
            for x in 0..width {
                let morton_index = Self::morton_order(x, y) as usize;
                let pixel_index = morton_index;
                let palette_index = data[pixel_index] as u16;

                let full_address = (palette_selector << 8) | palette_index;
                let pram_index = (full_address * 2) as usize;

                // Ensure pram_index is within bounds
                if pram_index + 1 >= pram.len() {
                    continue; // Skip if out-of-bounds
                }

                let color_4444 = u16::from_le_bytes([pram[pram_index], pram[pram_index + 1]]);

                let a = ((color_4444 >> 12) & 0xF) * 0xFF / 0xF;
                let r = ((color_4444 >> 8) & 0xF) * 0xFF / 0xF;
                let g = ((color_4444 >> 4) & 0xF) * 0xFF / 0xF;
                let b = (color_4444 & 0xF) * 0xFF / 0xF;

                let offset = (y * width + x) as usize * 4;
                rgba_data[offset] = b as u8;
                rgba_data[offset + 1] = g as u8;
                rgba_data[offset + 2] = r as u8;
                rgba_data[offset + 3] = a as u8;
            }
        }

        rgba_data
    }

    fn convert_twiddled_rgb1555_to_rgba8888(data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let mut rgba_data = Vec::with_capacity((width * height * 4) as usize);
        rgba_data.resize((width * height * 4) as usize, 0);

        for y in 0..height {
            for x in 0..width {
                let morton_index = Self::morton_order(x, y) as usize;
                let chunk_index = morton_index * 2;

                let chunk = &data[chunk_index..chunk_index + 2];
                let pixel = u16::from_le_bytes([chunk[0], chunk[1]]);
                let a = ((pixel >> 15) & 0x1) as u8;
                let r = ((pixel >> 10) & 0x1F) as u8;
                let g = ((pixel >> 5) & 0x1F) as u8;
                let b = (pixel & 0x1F) as u8;

                // Convert to 8-bit values
                let r = (r << 3) | (r >> 2);
                let g = (g << 3) | (g >> 2);
                let b = (b << 3) | (b >> 2);
                let a = if a == 1 { 255 } else { 0 };

                let linear_index = ((y * width + x) * 4) as usize;

                rgba_data[linear_index] = b;
                rgba_data[linear_index + 1] = g;
                rgba_data[linear_index + 2] = r;
                rgba_data[linear_index + 3] = a; // Use the alpha value correctly
            }
        }
        rgba_data
    }

    fn convert_rgb1555_to_rgba8888(data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let mut rgba_data = Vec::with_capacity((width * height * 4) as usize);
        rgba_data.resize((width * height * 4) as usize, 0);

        for i in 0..(width * height) {
            let chunk_index = (i * 2) as usize;
            if chunk_index + 1 >= data.len() {
                break; // Avoid out-of-bounds access
            }
            let chunk = &data[chunk_index..chunk_index + 2];
            let pixel = u16::from_le_bytes([chunk[0], chunk[1]]);

            let a = (((pixel >> 12) & 0xF) * 0x11) as u8; // Extract and scale alpha
            let r = (((pixel >> 8) & 0xF) * 0x11) as u8; // Extract and scale red
            let g = (((pixel >> 4) & 0xF) * 0x11) as u8; // Extract and scale green
            let b = ((pixel & 0xF) * 0x11) as u8; // Extract and scale blue

            let linear_index = (i * 4) as usize;
            rgba_data[linear_index] = b;
            rgba_data[linear_index + 1] = g;
            rgba_data[linear_index + 2] = r;
            rgba_data[linear_index + 3] = a; // Alpha channel
        }
        rgba_data
    }

    fn convert_rgb4444_to_argb8888(data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let mut argb_data = Vec::with_capacity((width * height * 4) as usize);
        for chunk in data.chunks_exact(2) {
            let word = u16::from_le_bytes([chunk[0], chunk[1]]);

            let a = (((word >> 12) & 0xF) * 0x11) as u8;
            let r = (((word >> 8) & 0xF) * 0x11) as u8;
            let g = (((word >> 4) & 0xF) * 0x11) as u8;
            let b = ((word & 0xF) * 0x11) as u8;

            argb_data.extend_from_slice(&[b, g, r, a]);
        }

        argb_data
    }

    fn convert_twiddled_rgb565_to_rgba8888(data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let mut rgba_data = Vec::with_capacity((width * height * 4) as usize);
        rgba_data.resize((width * height * 4) as usize, 0);

        for y in 0..height {
            for x in 0..width {
                let morton_index = Self::morton_order(x, y) as usize;
                let chunk_start = morton_index * 2;

                let chunk = &data[chunk_start..chunk_start + 2];
                let pixel = u16::from_le_bytes([chunk[0], chunk[1]]);
                let r = ((pixel >> 11) & 0x1F) as u8;
                let g = ((pixel >> 5) & 0x3F) as u8;
                let b = (pixel & 0x1F) as u8;

                // Convert to 8-bit values
                let r = (r << 3) | (r >> 2);
                let g = (g << 2) | (g >> 4);
                let b = (b << 3) | (b >> 2);

                let linear_index = ((y * width + x) * 4) as usize;

                rgba_data[linear_index] = b;
                rgba_data[linear_index + 1] = g;
                rgba_data[linear_index + 2] = r;
                rgba_data[linear_index + 3] = 255;
            }
        }
        rgba_data
    }

    fn convert_rgb565_to_rgba8888(data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let mut rgba_data = Vec::with_capacity((width * height * 4) as usize);
        rgba_data.resize((width * height * 4) as usize, 0);

        for y in 0..height {
            for x in 0..width {
                let index = ((y * width + x) * 2) as usize;

                if index + 1 >= data.len() {
                    continue;
                }

                let pixel = u16::from_le_bytes([data[index], data[index + 1]]);

                let r = ((pixel >> 11) & 0x1F) as u8;
                let g = ((pixel >> 5) & 0x3F) as u8;
                let b = (pixel & 0x1F) as u8;

                let r = (r << 3) | (r >> 2);
                let g = (g << 2) | (g >> 4);
                let b = (b << 3) | (b >> 2);

                let linear_index = ((y * width + x) * 4) as usize;

                rgba_data[linear_index] = b;
                rgba_data[linear_index + 1] = g;
                rgba_data[linear_index + 2] = r;
                rgba_data[linear_index + 3] = 255; // Fully opaque alpha channel
            }
        }
        rgba_data
    }

    pub fn notify_write(&mut self, addr: u32) {
        for (id, texture) in self.textures.iter_mut() {
            if texture.addr <= addr && addr < texture.end_addr {
                texture.dirty = true;
            }
        }
    }

    pub fn notify_paletted_write(&mut self, addr: u32) {
        for (id, texture) in self.textures.iter_mut() {
            if texture.palette_addr > 0 {
                texture.dirty = true;
            }
        }
    }

    pub fn upload_texture(
        &mut self,
        mut texture: Texture,
        vram: &mut [u8],
        pram: &mut [u8],
        texture_format: PvrTextureFmt,
        pixel_format: PvrPixelFmt,
    ) -> Result<TextureId, ()> {
        let texture_hash = Self::hash_texture(&texture, texture_format, pixel_format);
        let id = TextureId(texture_hash);

        if self.textures.contains_key(&id) {
            self.mark_texture(id);
            return Ok(id);
        }

        println!(
            "received metadata for {:?} {:#?}x{:#?} {:#?} {:#?}",
            id, texture.width, texture.height, texture_format, pixel_format
        );

        let end_addr = texture.addr
            + TextureMetadata::texture_size(
                texture.width,
                texture.height,
                pixel_format,
                texture_format,
            ) as u32;

        let metadata = TextureMetadata {
            id: id,
            width: texture.width,
            height: texture.height,
            generation: self.current_generation,
            texture_format,
            pixel_format,
            data: vec![],
            dirty: true,
            addr: texture.addr,
            palette_addr: texture.palette_addr,
            end_addr: texture.addr + end_addr,
            needs_upload: false,
        };

        self.textures.insert(id, metadata);
        self.realize_texture(id, vram, pram);

        Ok(id)
    }

    pub fn clear(&mut self) {
        self.data = vec![0; (self.width * self.height * 4) as usize]
    }

    pub fn mark_texture(&mut self, texture_id: TextureId) {
        self.textures.get_mut(&texture_id).unwrap().generation = self.current_generation;
    }

    pub fn realize_texture(&mut self, texture_id: TextureId, vram: &mut [u8], pram: &mut [u8]) {
        if let Some(texture) = self.textures.get_mut(&texture_id) {
            if texture.dirty {
                let texture_data = &vram[texture.addr as usize..(texture.end_addr) as usize];
                let converted_texture_data = match (texture.pixel_format, texture.texture_format) {
                    (PvrPixelFmt::Rgb565, PvrTextureFmt::Bitmap) => {
                        Self::convert_rgb565_to_rgba8888(
                            &texture_data,
                            texture.width,
                            texture.height,
                        )
                    }
                    (PvrPixelFmt::Rgb565, PvrTextureFmt::Twiddled) => {
                        Self::convert_twiddled_rgb565_to_rgba8888(
                            &texture_data,
                            texture.width,
                            texture.height,
                        )
                    }
                    (PvrPixelFmt::Argb4444, PvrTextureFmt::Bitmap) => {
                        Self::convert_rgb4444_to_argb8888(
                            &texture_data,
                            texture.width,
                            texture.height,
                        )
                    }
                    (PvrPixelFmt::Argb4444, PvrTextureFmt::Twiddled) => {
                        Self::convert_twiddled_rgb4444_to_argb8888(
                            &texture_data,
                            texture.width,
                            texture.height,
                        )
                    }
                    (PvrPixelFmt::Argb4444, PvrTextureFmt::TwiddledMipmaps) => {
                        Self::convert_twiddled_rgb4444_to_argb8888(
                            &texture_data,
                            texture.width,
                            texture.height,
                        )
                    }
                    (PvrPixelFmt::Argb1555, PvrTextureFmt::Bitmap) => {
                        Self::convert_rgb1555_to_rgba8888(
                            &texture_data,
                            texture.width,
                            texture.height,
                        )
                    }
                    (PvrPixelFmt::Argb1555, PvrTextureFmt::Twiddled) => {
                        Self::convert_twiddled_rgb1555_to_rgba8888(
                            &texture_data,
                            texture.width,
                            texture.height,
                        )
                    }
                    (PvrPixelFmt::Argb1555, PvrTextureFmt::TwiddledMipmaps) => {
                        Self::convert_twiddled_rgb1555_to_rgba8888(
                            &texture_data,
                            texture.width,
                            texture.height,
                        )
                    }
                    (PvrPixelFmt::EightBpp, PvrTextureFmt::Twiddled) => {
                        Self::convert_twiddled_8bpp_to_rgba8888(
                            &texture_data,
                            &pram,
                            texture.palette_addr,
                            texture.width,
                            texture.height,
                        )
                    }
                    _ => {
                        //println!("WARNING: unexpected texture format");
                        Self::convert_rgb565_to_rgba8888(
                            &texture_data,
                            texture.width,
                            texture.height,
                        )
                    }
                };

                texture.data = converted_texture_data;
                texture.dirty = false;
                texture.needs_upload = true;
            }

            texture.generation = self.current_generation;
        }
    }

    pub fn increment_generation(&mut self) {
        self.current_generation += 1;
    }
}
