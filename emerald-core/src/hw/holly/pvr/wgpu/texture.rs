pub struct Texture {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    pub sampler: wgpu::Sampler,
}

impl Texture {
    pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float; // 1.

    pub fn create_texture(device: &wgpu::Device, width: usize, height: usize) -> Texture {
        let size = wgpu::Extent3d {
            width: width as u32,
            height: height as u32,
            depth_or_array_layers: 1,
        };

        let desc = wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            // fixme: lower the texture modes
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: None,
            ..Default::default()
        });

        Texture {
            texture,
            view,
            sampler,
        }
    }

    pub fn create_texture_array(device: &wgpu::Device) -> [Texture; 8] {
        let x = [8, 16, 32, 64, 128, 256, 512, 1024].map(|i| {
            let size = wgpu::Extent3d {
                width: i,
                height: i,
                depth_or_array_layers: 256,
            };

            let desc = wgpu::TextureDescriptor {
                label: None,
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                // swap chain format??
                format: wgpu::TextureFormat::Bgra8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            };
            let texture = device.create_texture(&desc);

            // fixme: understand views + samplers
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                // fixme: lower the texture modes
                address_mode_u: wgpu::AddressMode::ClampToEdge,
                address_mode_v: wgpu::AddressMode::ClampToEdge,
                address_mode_w: wgpu::AddressMode::ClampToEdge,
                mag_filter: wgpu::FilterMode::Nearest,
                min_filter: wgpu::FilterMode::Nearest,
                mipmap_filter: wgpu::FilterMode::Nearest,
                compare: None,
                ..Default::default()
            });

            Texture {
                texture,
                view,
                sampler,
            }
        });

        x
    }

    pub fn create_depth_texture(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        label: &str,
    ) -> Self {
        let size = wgpu::Extent3d {
            width: config.width,
            height: config.height,
            depth_or_array_layers: 1,
        };

        let desc = wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::DEPTH_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        };
        let texture = device.create_texture(&desc);

        // fixme: understand views + samplers
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            // fixme: lower the texture modes
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            lod_min_clamp: 0.0,
            lod_max_clamp: 100.0,
            ..Default::default()
        });

        Self {
            texture,
            view,
            sampler,
        }
    }
}

#[derive(Default)]
pub struct TextureFreeList {
    bits: [u8; 32],
}

impl TextureFreeList {
    pub fn new() -> Self {
        TextureFreeList { bits: [0; 32] }
    }

    pub fn set(&mut self, index: usize) {
        let byte_index = index / 8;
        let bit_index = index % 8;
        self.bits[byte_index] |= 1 << bit_index;
    }

    pub fn unset(&mut self, index: usize) {
        let byte_index = index / 8;
        let bit_index = index % 8;
        self.bits[byte_index] &= !(1 << bit_index);
    }

    pub fn is_set(&self, index: usize) -> bool {
        let byte_index = index / 8;
        let bit_index = index % 8;
        self.bits[byte_index] & (1 << bit_index) != 0
    }

    pub fn clear(&mut self) {
        for byte in self.bits.iter_mut() {
            *byte = 0;
        }
    }

    pub fn find_free(&self) -> Option<usize> {
        for (byte_index, &byte) in self.bits.iter().enumerate() {
            if byte != 0xFF {
                for bit_index in 0..8 {
                    if byte & (1 << bit_index) == 0 {
                        return Some(byte_index * 8 + bit_index);
                    }
                }
            }
        }
        None
    }
}
