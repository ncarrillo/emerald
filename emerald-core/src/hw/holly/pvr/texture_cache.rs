use std::{
    collections::HashMap,
    hash::{DefaultHasher, Hash, Hasher},
};

use super::TextureFormat;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TextureId(u64);

pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct TextureMetadata {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub generation: u64,
    pub format: TextureFormat,
    pub data: Vec<u8>,
}

#[derive(Copy, Clone, Debug)]
pub struct TextureEntry {
    pub width: u32,
    pub height: u32,
}

pub struct TextureAtlas {
    width: u32,
    height: u32,
    pub data: Vec<u8>,
    textures: HashMap<TextureId, TextureMetadata>,
    current_x: u32,
    current_y: u32,
    max_row_height: u32,
    current_generation: u64,
}

impl TextureAtlas {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            data: vec![0; (width * height * 4) as usize],
            textures: HashMap::new(),
            current_x: 0,
            current_y: 0,
            max_row_height: 0,
            current_generation: 0,
        }
    }

    fn hash_texture(texture: &Texture) -> u64 {
        let mut hasher = DefaultHasher::new();
        texture.data.hash(&mut hasher);
        texture.width.hash(&mut hasher);
        texture.height.hash(&mut hasher);
        hasher.finish()
    }

    fn convert_rgb565_to_rgba8888(data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let mut rgba_data = Vec::with_capacity((width * height * 4) as usize);
        for chunk in data.chunks(2) {
            let pixel = u16::from_le_bytes([chunk[0], chunk[1]]);
            let r = ((pixel >> 11) & 0x1F) as u8;
            let g = ((pixel >> 5) & 0x3F) as u8;
            let b = (pixel & 0x1F) as u8;

            // Convert to 8-bit values
            let r = (r << 3) | (r >> 2);
            let g = (g << 2) | (g >> 4);
            let b = (b << 3) | (b >> 2);

            rgba_data.extend_from_slice(&[r, g, b, 0]);
        }
        rgba_data
    }

    fn convert_rgb4444_to_rgba8888(data: &[u8], width: u32, height: u32) -> Vec<u8> {
        let mut rgba_data = Vec::with_capacity((width * height * 4) as usize);
        for chunk in data.chunks(2) {
            let pixel = u16::from_le_bytes([chunk[0], chunk[1]]);
            let r = ((pixel >> 12) & 0xF) as u8;
            let g = ((pixel >> 8) & 0xF) as u8;
            let b = ((pixel >> 4) & 0xF) as u8;
            let a = (pixel & 0xF) as u8;

            let r = (r << 4) | r;
            let g = (g << 4) | g;
            let b = (b << 4) | b;
            let a = (a << 4) | a;

            rgba_data.extend_from_slice(&[r, g, b, a]);
        }
        rgba_data
    }

    pub fn upload_texture(
        &mut self,
        texture: Texture,
        format: TextureFormat,
    ) -> Result<TextureId, ()> {
        let texture_data = match format {
            TextureFormat::Rgb565 => {
                Self::convert_rgb565_to_rgba8888(&texture.data, texture.width, texture.height)
            }
            TextureFormat::Rgb4444 => {
                Self::convert_rgb4444_to_rgba8888(&texture.data, texture.width, texture.height)
            }
            _ => unimplemented!(),
        };

        let converted_texture = Texture {
            width: texture.width,
            height: texture.height,
            data: texture_data.clone(),
        };

        let texture_hash = Self::hash_texture(&converted_texture);
        let id = TextureId(texture_hash);

        if self.textures.contains_key(&id) {
            return Ok(id);
        }

        let (x, y) = self.find_space_for_texture(converted_texture.width, converted_texture.height);
        self.copy_texture_into_atlas(&converted_texture, x, y);
        let metadata = TextureMetadata {
            x,
            y,
            width: converted_texture.width,
            height: converted_texture.height,
            generation: self.current_generation,
            format: format,
            data: texture_data,
        };

        self.textures.insert(id, metadata);
        Ok(id)
    }

    fn copy_texture_into_atlas(&mut self, texture: &Texture, x: u32, y: u32) {
        for i in 0..texture.height {
            let atlas_start_index = ((y + i) * self.width + x) * 4;
            let texture_start_index = (i * texture.width) * 4;
            self.data[atlas_start_index as usize..(atlas_start_index + texture.width * 4) as usize]
                .copy_from_slice(
                    &texture.data[texture_start_index as usize
                        ..(texture_start_index + texture.width * 4) as usize],
                );
        }
    }

    fn find_space_for_texture(&mut self, width: u32, height: u32) -> (u32, u32) {
        if self.current_x + width > self.width {
            self.current_x = 0;
            self.current_y += self.max_row_height;
            self.max_row_height = 0;
        }

        if self.current_y + height > self.height {
            self.evict_stale_textures(1);
            if self.current_y + height > self.height {
                panic!(
                    "texture cache: atlas is full even after eviction! requested space for incoming {} {} entry.",
                    width, height
                );
            }
        }

        self.max_row_height = self.max_row_height.max(height);

        let x = self.current_x;
        let y = self.current_y;

        self.current_x += width;

        (x, y)
    }

    pub fn get_texture_slice(&mut self, texture_id: TextureId) -> Option<(TextureEntry, &[u8])> {
        if let Some(metadata) = self.textures.get_mut(&texture_id) {
            metadata.generation = self.current_generation;
            let TextureMetadata {
                x,
                y,
                width,
                height,
                ..
            } = *metadata;
            let start = ((y * self.width + x) * 4) as usize;
            let end = (((y + height - 1) * self.width + x + width) * 4) as usize;

            Some((
                TextureEntry {
                    width: metadata.width,
                    height: metadata.height,
                },
                &metadata.data,
            ))
        } else {
            None
        }
    }

    pub fn increment_generation(&mut self) {
        self.current_generation += 1;
    }

    pub fn evict_stale_textures(&mut self, threshold: u64) {
        self.textures
            .retain(|_, metadata| self.current_generation - metadata.generation <= threshold);
    }
}
