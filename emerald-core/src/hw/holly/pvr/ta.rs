use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(Clone, Copy)]
pub union TextureControlWord {
    pub full: u32,
    pub rgb_yuv_bumpmap: RGBYUVBumpmap,
    pub palette: Palette,
}

impl std::fmt::Debug for TextureControlWord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe {
            f.debug_struct("TextureControlWord")
                .field("full", &self.full)
                .field("rgb_yuv_bumpmap", &self.rgb_yuv_bumpmap)
                .field("palette", &self.palette)
                .finish()
        }
    }
}

impl TextureControlWord {
    pub fn pixel_fmt(&self) -> PvrPixelFmt {
        unsafe { PvrPixelFmt::from_u32((self.full & 0x38000000) >> 27) }
    }

    pub fn texture_addr(&self) -> u32 {
        unsafe { (self.full & 0x1FFFFF) >> 0 }
    }

    pub fn vq_compressed(&self) -> bool {
        unsafe { (self.full & 0x40000000) != 0 }
    }

    fn twiddled(&self) -> bool {
        return !self.scan_order()
            || self.pixel_fmt() == PvrPixelFmt::EightBpp
            || self.pixel_fmt() == PvrPixelFmt::FourBpp;
    }

    pub fn scan_order(&self) -> bool {
        unsafe { (self.full & 0x04000000) != 0 }
    }

    pub fn mip_mapped(&self) -> bool {
        unsafe { (self.full & 0x80000000) != 0 }
    }

    pub fn texture_fmt(&self) -> PvrTextureFmt {
        let compressed = self.vq_compressed();
        let twiddled = self.twiddled();
        let mipmaps = self.mip_mapped();

        match (self.pixel_fmt(), compressed, twiddled, mipmaps) {
            (PvrPixelFmt::FourBpp, false, false, true) => PvrTextureFmt::Palette4BppMipmaps,
            (PvrPixelFmt::FourBpp, false, false, false) => PvrTextureFmt::Palette4BppMipmaps,
            (PvrPixelFmt::EightBpp, false, false, true) => PvrTextureFmt::Palette8BppMipmaps,
            (PvrPixelFmt::EightBpp, false, false, false) => PvrTextureFmt::Palette8BppMipmaps,

            (_, true, false, true) => PvrTextureFmt::VqMipmaps,
            (_, true, false, false) => PvrTextureFmt::Vq,
            (_, false, true, true) => PvrTextureFmt::TwiddledMipmaps,
            (_, false, true, false) => PvrTextureFmt::Twiddled,
            _ => PvrTextureFmt::Bitmap,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct RGBYUVBumpmap {
    pub bits: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct Palette {
    pub bits: u32,
}

impl RGBYUVBumpmap {
    pub fn new(value: u32) -> Self {
        Self { bits: value }
    }

    pub fn texture_addr(&self) -> u32 {
        (self.bits & 0x1FFFFF) >> 0
    }

    pub fn reserved(&self) -> u32 {
        (self.bits & 0x01E00000) >> 21
    }

    pub fn stride_select(&self) -> bool {
        (self.bits & 0x02000000) != 0
    }

    pub fn scan_order(&self) -> bool {
        (self.bits & 0x04000000) != 0
    }

    pub fn pixel_fmt(&self) -> u32 {
        (self.bits & 0x38000000) >> 27
    }

    pub fn vq_compressed(&self) -> bool {
        (self.bits & 0x40000000) != 0
    }

    pub fn mip_mapped(&self) -> bool {
        (self.bits & 0x80000000) != 0
    }
}

impl Palette {
    pub fn new(value: u32) -> Self {
        Self { bits: value }
    }

    pub fn texture_addr(&self) -> u32 {
        (self.bits & 0x1FFFFF) >> 0
    }

    pub fn palette_selector(&self) -> u32 {
        (self.bits & 0x03E00000) >> 21
    }

    pub fn pixel_fmt(&self) -> u32 {
        (self.bits & 0x38000000) >> 27
    }

    pub fn vq_compressed(&self) -> bool {
        (self.bits & 0x40000000) != 0
    }

    pub fn mip_mapped(&self) -> bool {
        (self.bits & 0x80000000) != 0
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ImageSynthesisProcessorWord {
    pub full: u32,
}

impl ImageSynthesisProcessorWord {
    pub fn new(value: u32) -> Self {
        Self { full: value }
    }

    pub fn dcalc_ctrl(&self) -> bool {
        (self.full & 0x00100000) != 0
    }

    pub fn cache_bypass(&self) -> bool {
        (self.full & 0x00200000) != 0
    }

    pub fn uv_16bit(&self) -> bool {
        (self.full & 0x00400000) != 0
    }

    pub fn gouraud(&self) -> bool {
        (self.full & 0x00800000) != 0
    }

    pub fn offset(&self) -> bool {
        (self.full & 0x01000000) != 0
    }

    pub fn texture(&self) -> bool {
        (self.full & 0x02000000) != 0
    }

    pub fn z_write_disable(&self) -> bool {
        (self.full & 0x04000000) != 0
    }

    pub fn culling_mode(&self) -> u32 {
        (self.full & 0x18000000) >> 27
    }

    pub fn depth_compare_mode(&self) -> u32 {
        (self.full & 0xE0000000) >> 29
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct ParameterControlWord {
    pub full: u32,
}

impl ParameterControlWord {
    pub fn new(value: u32) -> Self {
        Self { full: value }
    }

    pub fn expected_words(&self, vert_type: VertexType) -> usize {
        match self.para_type() {
            ParameterType::EndOfList => 1,
            ParameterType::UserTileClip => 1,
            ParameterType::ObjectList => 1,
            ParameterType::PolyOrVol
                if (vert_type == VertexType::Type0
                    || vert_type == VertexType::Type1
                    || vert_type == VertexType::Type3) =>
            {
                1
            }
            ParameterType::PolyOrVol => 2,
            ParameterType::Sprite => 1,
            ParameterType::Vertex => {
                if vert_type == VertexType::Type0
                    || vert_type == VertexType::Type1
                    || vert_type == VertexType::Type2
                    || vert_type == VertexType::Type3
                    || vert_type == VertexType::Type4
                    || vert_type == VertexType::Type7
                    || vert_type == VertexType::Type8
                    || vert_type == VertexType::Type9
                    || vert_type == VertexType::Type10
                {
                    1
                } else {
                    2
                }
            }
            _ => 0,
        }
    }

    pub fn vert_type(&self) -> VertexType {
        if self.list_type() == PvrListType::OpaqueModVol
            || self.list_type() == PvrListType::TranslucentModVol
        {
            return VertexType::ModVol;
        }

        if self.para_type() == ParameterType::Sprite {
            return if self.texture() {
                VertexType::SpriteType1
            } else {
                VertexType::SpriteType0
            };
        }

        if self.volume() {
            if self.texture() {
                if self.col_type() == 0 {
                    return if self.uv_16bit() {
                        VertexType::Type12
                    } else {
                        VertexType::Type11
                    };
                }
                if self.col_type() == 2 || self.col_type() == 3 {
                    return if self.uv_16bit() {
                        VertexType::Type14
                    } else {
                        VertexType::Type13
                    };
                }
            }

            if self.col_type() == 0 {
                return VertexType::Type9;
            }

            if self.col_type() == 2 || self.col_type() == 3 {
                return VertexType::Type10;
            }
        }

        if self.texture() {
            if self.col_type() == 0 {
                return if self.uv_16bit() {
                    VertexType::Type4
                } else {
                    VertexType::Type3
                };
            }

            if self.col_type() == 1 {
                return if self.uv_16bit() {
                    VertexType::Type6
                } else {
                    VertexType::Type5
                };
            }

            if self.col_type() == 2 || self.col_type() == 3 {
                return if self.uv_16bit() {
                    VertexType::Type8
                } else {
                    VertexType::Type7
                };
            }
        }

        if self.col_type() == 0 {
            return VertexType::Type0;
        }
        if self.col_type() == 1 {
            return VertexType::Type1;
        }
        if self.col_type() == 2 || self.col_type() == 3 {
            return VertexType::Type2;
        }

        return VertexType::Type0;
    }

    pub fn poly_type(&self) -> PolygonType {
        if self.list_type() == PvrListType::OpaqueModVol
            || self.list_type() == PvrListType::TranslucentModVol
        {
            return PolygonType::ModVol;
        }

        if self.para_type() == ParameterType::Sprite {
            return PolygonType::Sprite;
        }

        if self.volume() {
            if self.col_type() == 0 || self.col_type() == 3 {
                return PolygonType::Type3;
            }

            if self.col_type() == 2 {
                return PolygonType::Type4;
            }
        }

        if self.col_type() == 0 || self.col_type() == 1 || self.col_type() == 3 {
            return PolygonType::Type0;
        }

        if self.col_type() == 2 && self.texture() && !self.offset() {
            return PolygonType::Type1;
        }

        if self.col_type() == 2 && self.texture() && self.offset() {
            return PolygonType::Type2;
        }

        if self.col_type() == 2 && !self.texture() {
            return PolygonType::Type1;
        }

        return PolygonType::Type0;
    }

    // Object control
    pub fn uv_16bit(&self) -> bool {
        (self.full & 0x00000001) != 0
    }

    pub fn gouraud(&self) -> bool {
        (self.full & 0x00000002) != 0
    }

    pub fn offset(&self) -> bool {
        (self.full & 0x00000004) != 0
    }

    pub fn texture(&self) -> bool {
        (self.full & 0x00000008) != 0
    }

    pub fn col_type(&self) -> u32 {
        (self.full & 0x00000030) >> 4
    }

    pub fn volume(&self) -> bool {
        (self.full & 0x00000040) != 0
    }

    pub fn shadow(&self) -> bool {
        (self.full & 0x00000080) != 0
    }

    pub fn reserved0(&self) -> u32 {
        (self.full & 0x0000FF00) >> 8
    }

    // Group control
    pub fn user_clip(&self) -> u32 {
        (self.full & 0x00030000) >> 16
    }

    pub fn strip_len(&self) -> u32 {
        (self.full & 0x000C0000) >> 18
    }

    pub fn reserved1(&self) -> u32 {
        (self.full & 0x00700000) >> 20
    }

    pub fn group_en(&self) -> bool {
        (self.full & 0x00800000) != 0
    }

    // Para control
    pub fn list_type(&self) -> PvrListType {
        PvrListType::from_u32((self.full & 0x07000000) >> 24)
    }

    pub fn reserved2(&self) -> u32 {
        (self.full & 0x08000000) >> 27
    }

    pub fn end_of_strip(&self) -> bool {
        (self.full & 0x10000000) != 0
    }

    pub fn para_type(&self) -> ParameterType {
        ParameterType::from_u32((self.full & 0xE0000000) >> 29)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct TextureShadingProcessorWord {
    pub full: u32,
}

impl TextureShadingProcessorWord {
    pub fn new(value: u32) -> Self {
        Self { full: value }
    }

    pub fn texture_v_size(&self) -> u32 {
        (self.full & 0x00000007) >> 0
    }

    pub fn texture_u_size(&self) -> u32 {
        (self.full & 0x00000038) >> 3
    }

    pub fn texture_shading_instr(&self) -> u32 {
        (self.full & 0x000000C0) >> 6
    }

    pub fn mipmap_d_adjust(&self) -> u32 {
        (self.full & 0x00000F00) >> 8
    }

    pub fn super_sample_texture(&self) -> bool {
        (self.full & 0x00001000) != 0
    }

    pub fn filter_mode(&self) -> u32 {
        (self.full & 0x00006000) >> 13
    }

    pub fn clamp_v(&self) -> bool {
        (self.full & 0x00008000) != 0
    }

    pub fn clamp_u(&self) -> bool {
        (self.full & 0x00010000) != 0
    }

    pub fn flip_v(&self) -> bool {
        (self.full & 0x00020000) != 0
    }

    pub fn flip_u(&self) -> bool {
        (self.full & 0x00040000) != 0
    }

    pub fn ignore_tex_alpha(&self) -> bool {
        (self.full & 0x00080000) != 0
    }

    pub fn use_alpha(&self) -> bool {
        (self.full & 0x00100000) != 0
    }

    pub fn color_clamp(&self) -> bool {
        (self.full & 0x00200000) != 0
    }

    pub fn fog_control(&self) -> u32 {
        (self.full & 0x00C00000) >> 22
    }

    pub fn dst_select(&self) -> bool {
        (self.full & 0x01000000) != 0
    }

    pub fn src_select(&self) -> bool {
        (self.full & 0x02000000) != 0
    }

    pub fn dst_alpha_instr(&self) -> u32 {
        (self.full & 0x1C000000) >> 26
    }

    pub fn src_alpha_instr(&self) -> u32 {
        (self.full & 0xE0000000) >> 29
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum VertexType {
    Type0 = 0,
    Type1 = 1,
    Type2 = 2,
    Type3 = 3,
    Type4 = 4,
    Type5 = 5,
    Type6 = 6,
    Type7 = 7,
    Type8 = 8,
    Type9 = 9,
    Type10 = 10,
    Type11 = 11,
    Type12 = 12,
    Type13 = 13,
    Type14 = 14,
    SpriteType0 = 15,
    SpriteType1 = 16,
    ModVol = 17, // I think??
}

impl VertexType {
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => VertexType::Type0,
            1 => VertexType::Type1,
            2 => VertexType::Type2,
            3 => VertexType::Type3,
            4 => VertexType::Type4,
            5 => VertexType::Type5,
            6 => VertexType::Type6,
            7 => VertexType::Type7,
            8 => VertexType::Type8,
            9 => VertexType::Type9,
            10 => VertexType::Type10,
            11 => VertexType::Type11,
            12 => VertexType::Type12,
            13 => VertexType::Type13,
            14 => VertexType::Type14,
            15 => VertexType::SpriteType0,
            16 => VertexType::SpriteType1,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum PolygonType {
    Type0 = 0,
    Type1 = 1,
    Type2 = 2,
    Type3 = 3,
    Type4 = 4,
    Sprite = 5,
    ModVol = 6,
}

impl PolygonType {
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => PolygonType::Type0,
            1 => PolygonType::Type1,
            2 => PolygonType::Type2,
            3 => PolygonType::Type3,
            4 => PolygonType::Type4,
            _ => unreachable!(),
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union PolyParam {
    pub type0: PolygonType0,
    pub type1: PolygonType1,
    pub type2: PolygonType2,
    pub type3: PolygonType3,
    pub type4: PolygonType4,
    pub sprite: Sprite,
    pub modvol: ModVol,
    pub pcw: ParameterControlWord,
    pub full: [u32; 16], // Ensure it can accommodate the largest variant
}

impl std::fmt::Debug for PolyParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe {
            f.debug_struct("PolyParam")
                .field("type0", &self.type0)
                .field("type1", &self.type1)
                .field("type2", &self.type2)
                .field("type3", &self.type3)
                .field("type4", &self.type4)
                .field("sprite", &self.sprite)
                .field("modvol", &self.modvol)
                .field("full", &self.full)
                .finish()
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PolygonType0 {
    pub pcw: ParameterControlWord,
    pub isp: ImageSynthesisProcessorWord,
    pub tsp: TextureShadingProcessorWord,
    pub tcw: TextureControlWord,
    pub ignore_0: u32,
    pub ignore_1: u32,
    pub sdma_data_size: u32,
    pub sdma_next_addr: u32,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PolygonType1 {
    pub pcw: ParameterControlWord,
    pub isp: ImageSynthesisProcessorWord,
    pub tsp: TextureShadingProcessorWord,
    pub tcw: TextureControlWord,
    pub face_color_a: f32,
    pub face_color_r: f32,
    pub face_color_g: f32,
    pub face_color_b: f32,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PolygonType2 {
    pub pcw: ParameterControlWord,
    pub isp: ImageSynthesisProcessorWord,
    pub tsp: TextureShadingProcessorWord,
    pub tcw: TextureControlWord,
    pub ignore_0: u32,
    pub ignore_1: u32,
    pub sdma_data_size: u32,
    pub sdma_next_addr: u32,
    pub face_color_a: f32,
    pub face_color_r: f32,
    pub face_color_g: f32,
    pub face_color_b: f32,
    pub face_offset_color_a: f32,
    pub face_offset_color_r: f32,
    pub face_offset_color_g: f32,
    pub face_offset_color_b: f32,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PolygonType3 {
    pub pcw: ParameterControlWord,
    pub isp: ImageSynthesisProcessorWord,
    pub tsp0: TextureShadingProcessorWord,
    pub tcw0: TextureControlWord,
    pub tsp1: TextureShadingProcessorWord,
    pub tcw1: TextureControlWord,
    pub sdma_data_size: u32,
    pub sdma_next_addr: u32,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct PolygonType4 {
    pub pcw: ParameterControlWord,
    pub isp: ImageSynthesisProcessorWord,
    pub tsp0: TextureShadingProcessorWord,
    pub tcw0: TextureControlWord,
    pub tsp1: TextureShadingProcessorWord,
    pub tcw1: TextureControlWord,
    pub sdma_data_size: u32,
    pub sdma_next_addr: u32,
    pub face_color_a_0: f32,
    pub face_color_r_0: f32,
    pub face_color_g_0: f32,
    pub face_color_b_0: f32,
    pub face_color_a_1: f32,
    pub face_color_r_1: f32,
    pub face_color_g_1: f32,
    pub face_color_b_1: f32,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct Sprite {
    pub pcw: ParameterControlWord,
    pub isp: ImageSynthesisProcessorWord,
    pub tsp: TextureShadingProcessorWord,
    pub tcw: TextureControlWord,
    pub base_color: u32,
    pub offset_color: u32,
    pub sdma_data_size: u32,
    pub sdma_next_addr: u32,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct ModVol {
    pub pcw: ParameterControlWord,
    pub isp: ImageSynthesisProcessorWord,
    pub reserved: [u32; 6],
}

impl PolyParam {
    pub fn new(value: [u32; 16]) -> Self {
        Self { full: value }
    }

    // fix once we know more
    pub fn new8(value: [u32; 8]) -> Self {
        let mut full_value = [0u32; 16];
        full_value[..8].copy_from_slice(&value);
        Self { full: full_value }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
pub union VertexParam {
    pub type0: VertexType0,
    pub type1: VertexType1,
    pub type2: VertexType2,
    pub type3: VertexType3,
    pub type4: VertexType4,
    pub type5: VertexType5,
    pub type6: VertexType6,
    pub type7: VertexType7,
    pub type8: VertexType8,
    pub type9: VertexType9,
    pub type10: VertexType10,
    pub type11: VertexType11,
    pub type12: VertexType12,
    pub type13: VertexType13,
    pub type14: VertexType14,
    pub sprite0: SpriteType0,
    pub sprite1: SpriteType1,
    pub pcw: ParameterControlWord,
    pub full: [u32; 16], // Ensure it can accommodate the largest variant
}

impl std::fmt::Debug for VertexParam {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        unsafe {
            f.debug_struct("VertexParam")
                .field(
                    "",
                    match self.pcw.vert_type() {
                        VertexType::Type0 => &self.type0,
                        VertexType::Type1 => &self.type1,
                        VertexType::Type2 => &self.type2,
                        VertexType::Type3 => &self.type3,
                        VertexType::Type4 => &self.type4,
                        VertexType::Type5 => &self.type5,
                        VertexType::Type6 => &self.type6,
                        VertexType::Type7 => &self.type7,
                        VertexType::Type8 => &self.type8,
                        VertexType::Type9 => &self.type9,
                        VertexType::Type10 => &self.type10,
                        VertexType::Type11 => &self.type11,
                        VertexType::Type12 => &self.type12,
                        VertexType::Type13 => &self.type13,
                        VertexType::Type14 => &self.type14,
                        VertexType::SpriteType0 => &self.sprite0,
                        VertexType::SpriteType1 => &self.sprite1,
                        _ => unreachable!(),
                    },
                )
                .finish()
        }
    }
}

impl VertexParam {
    pub fn new(value: [u32; 16]) -> Self {
        Self { full: value }
    }

    pub fn new_short(value: [u32; 8]) -> Self {
        let mut full_value = [0u32; 16];
        full_value[..8].copy_from_slice(&value);
        Self { full: full_value }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType0 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub ignore_0: u32,
    pub ignore_1: u32,
    pub base_color: u32,
    pub ignore_2: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType1 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub base_color_a: f32,
    pub base_color_r: f32,
    pub base_color_g: f32,
    pub base_color_b: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType2 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub ignore_0: u32,
    pub ignore_1: u32,
    pub base_intensity: f32,
    pub ignore_2: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType3 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub uv: [f32; 2],
    pub base_color: u32,
    pub offset_color: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType4 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub uv: [u16; 2],
    pub ignore_0: u32,
    pub base_color: u32,
    pub offset_color: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType5 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub uv: [f32; 2],
    pub ignore_0: u32,
    pub ignore_1: u32,
    pub base_color_a: f32,
    pub base_color_r: f32,
    pub base_color_g: f32,
    pub base_color_b: f32,
    pub offset_color_a: f32,
    pub offset_color_r: f32,
    pub offset_color_g: f32,
    pub offset_color_b: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType6 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub uv: [u16; 2],
    pub ignore_0: u32,
    pub ignore_1: u32,
    pub ignore_2: u32,
    pub base_color_a: f32,
    pub base_color_r: f32,
    pub base_color_g: f32,
    pub base_color_b: f32,
    pub offset_color_a: f32,
    pub offset_color_r: f32,
    pub offset_color_g: f32,
    pub offset_color_b: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType7 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub uv: [f32; 2],
    pub base_intensity: f32,
    pub offset_intensity: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType8 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub uv: [u16; 2],
    pub ignore_0: u32,
    pub base_intensity: f32,
    pub offset_intensity: f32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType9 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub base_color_0: u32,
    pub base_color_1: u32,
    pub ignore_0: u32,
    pub ignore_1: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType10 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub base_intensity_0: f32,
    pub base_intensity_1: f32,
    pub ignore_0: u32,
    pub ignore_1: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType11 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub uv_0: [f32; 2],
    pub base_color_0: u32,
    pub offset_color_0: u32,
    pub uv_1: [f32; 2],
    pub base_color_1: u32,
    pub offset_color_1: u32,
    pub ignore_0: u32,
    pub ignore_1: u32,
    pub ignore_2: u32,
    pub ignore_3: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType12 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub vu_0: [u16; 2],
    pub ignore_0: u32,
    pub base_color_0: u32,
    pub offset_color_0: u32,
    pub vu_1: [u16; 2],
    pub ignore_1: u32,
    pub base_color_1: u32,
    pub offset_color_1: u32,
    pub ignore_2: u32,
    pub ignore_3: u32,
    pub ignore_4: u32,
    pub ignore_5: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType13 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub uv_0: [f32; 2],
    pub base_intensity_0: f32,
    pub offset_intensity_0: f32,
    pub uv_1: [f32; 2],
    pub base_intensity_1: f32,
    pub offset_intensity_1: f32,
    pub ignore_0: u32,
    pub ignore_1: u32,
    pub ignore_2: u32,
    pub ignore_3: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct VertexType14 {
    pub pcw: ParameterControlWord,
    pub xyz: [f32; 3],
    pub vu_0: [u16; 2],
    pub ignore_0: u32,
    pub base_intensity_0: f32,
    pub offset_intensity_0: f32,
    pub vu_1: [u16; 2],
    pub ignore_1: u32,
    pub base_intensity_1: f32,
    pub offset_intensity_1: f32,
    pub ignore_2: u32,
    pub ignore_3: u32,
    pub ignore_4: u32,
    pub ignore_5: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct SpriteType0 {
    pub pcw: ParameterControlWord,
    pub xyz: [[f32; 3]; 4],
    pub ignore_0: u32,
    pub ignore_1: u32,
    pub ignore_2: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[repr(C)]
pub struct SpriteType1 {
    pub pcw: ParameterControlWord,
    pub xyz: [[f32; 3]; 4],
    pub uv: [[u16; 2]; 3],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum ParameterType {
    /* control params */
    EndOfList = 0,
    UserTileClip = 1,
    ObjectList = 2,
    Reserved0 = 3,
    /* global params */
    PolyOrVol = 4,
    Sprite = 5,
    Reserved1 = 6,
    /* vertex params */
    Vertex = 7,
}

impl ParameterType {
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => ParameterType::EndOfList,
            1 => ParameterType::UserTileClip,
            2 => ParameterType::ObjectList,
            3 => ParameterType::Reserved0,
            4 => ParameterType::PolyOrVol,
            5 => ParameterType::Sprite,
            6 => ParameterType::Reserved1,
            7 => ParameterType::Vertex,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum PvrListType {
    Opaque = 0,
    OpaqueModVol = 1,
    Translucent = 2,
    TranslucentModVol = 3,
    PunchThrough = 4,
    NumLists = 5,
}

impl PvrListType {
    pub fn from_u32(value: u32) -> Self {
        match value {
            0 => PvrListType::Opaque,
            1 => PvrListType::OpaqueModVol,
            2 => PvrListType::Translucent,
            3 => PvrListType::TranslucentModVol,
            4 => PvrListType::PunchThrough,
            5 => PvrListType::NumLists,
            _ => PvrListType::Opaque,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum PvrPixelFmt {
    Argb1555 = 0x0,
    Rgb565,
    Argb4444,
    Yuv422,
    Bumpmap,
    FourBpp,
    EightBpp,
    Reserved,
}

impl PvrPixelFmt {
    pub fn from_u32(value: u32) -> Self {
        match value {
            0x0 => PvrPixelFmt::Argb1555,
            0x1 => PvrPixelFmt::Rgb565,
            0x2 => PvrPixelFmt::Argb4444,
            0x3 => PvrPixelFmt::Yuv422,
            0x4 => PvrPixelFmt::Bumpmap,
            0x5 => PvrPixelFmt::FourBpp,
            0x6 => PvrPixelFmt::EightBpp,
            0x7 => PvrPixelFmt::Reserved,
            _ => unreachable!(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u32)]
pub enum PvrTextureFmt {
    Invalid = 0x0,
    Twiddled = 0x1,
    TwiddledMipmaps = 0x2,
    Vq = 0x3,
    VqMipmaps = 0x4,
    Palette4Bpp = 0x5,
    Palette4BppMipmaps = 0x6,
    Palette8Bpp = 0x7,
    Palette8BppMipmaps = 0x8,
    BitmapRect = 0x9,
    Bitmap = 0xb,
    TwiddledRect = 0xd,
}

impl PvrTextureFmt {
    pub fn from_u32(value: u32) -> Self {
        match value {
            0x0 => PvrTextureFmt::Invalid,
            0x1 => PvrTextureFmt::Twiddled,
            0x2 => PvrTextureFmt::TwiddledMipmaps,
            0x3 => PvrTextureFmt::Vq,
            0x4 => PvrTextureFmt::VqMipmaps,
            0x5 => PvrTextureFmt::Palette4Bpp,
            0x6 => PvrTextureFmt::Palette4BppMipmaps,
            0x7 => PvrTextureFmt::Palette8Bpp,
            0x8 => PvrTextureFmt::Palette8BppMipmaps,
            0x9 => PvrTextureFmt::BitmapRect,
            0xb => PvrTextureFmt::Bitmap,
            0xd => PvrTextureFmt::TwiddledRect,
            _ => unreachable!(),
        }
    }
}
