use crate::hw::holly::pvr::ta::VertexType;
use serde::{Deserialize, Serialize};
use std::{cmp::max, mem};

use super::{
    ta::{
        PolyParam, PolygonType, PvrPixelFmt, PvrTextureFmt, TextureShadingProcessorWord,
        VertexParam,
    },
    texture_cache::{Texture, TextureAtlas, TextureId},
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum DisplayListItem {
    Polygon(PolygonDisplayItem),
    UploadTexture {
        texture: Texture,
        pixel_format: PvrPixelFmt,
        texture_format: PvrTextureFmt,
    },
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PolygonDisplayItem {
    pub texture_id: Option<TextureId>,
    pub vert_type: VertexType,
    pub poly_type: PolygonType,
    pub tsp: TextureShadingProcessorWord,
    pub starting_vert: usize,
    pub vert_len: usize,
    pub palette_selector: u32,
    pub face_color: [u8; 4],
}

#[derive(Copy, Default, Clone, Debug, Serialize, Deserialize)]
pub struct VertexDefinition {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub u: f32,
    pub v: f32,
    pub end_of_strip: bool,
    pub color: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayList {
    pub items: Vec<DisplayListItem>,
    pub verts: Vec<VertexDefinition>,
}

pub struct DisplayListId(pub u32);

impl DisplayList {
    pub fn new(items: Vec<DisplayListItem>) -> Self {
        Self {
            items,
            verts: vec![],
        }
    }

    pub fn items(&self) -> &[DisplayListItem] {
        &self.items
    }

    pub fn verts(&self) -> &[VertexDefinition] {
        &self.verts
    }

    pub fn items_mut(&mut self) -> &mut [DisplayListItem] {
        &mut self.items
    }
}

#[derive(Clone, Debug)]
pub struct DisplayListBuilder {
    // the current polygon
    pub current_poly: Option<PolygonDisplayItem>,
    prev_poly: Option<PolygonDisplayItem>,
    prev_vert: Option<VertexParam>,

    // pending display list items + verts
    pub pending_list: Vec<DisplayListItem>,
    pub verts: Vec<VertexDefinition>,
}

impl DisplayListBuilder {
    pub fn new() -> Self {
        Self {
            pending_list: vec![],
            verts: vec![],

            current_poly: None,
            prev_poly: None,
            prev_vert: None,
        }
    }

    #[inline]
    fn fmulu8(a: u8, b: u8) -> u8 {
        ((a as u32 * b as u32) / 255) as u8
    }

    pub fn push_poly(&mut self, poly: PolyParam) {
        if self.current_poly.is_some() {
            self.commit_pending_polygon();
            assert!(self.current_poly.is_none());
        }

        self.prev_vert = None;

        unsafe {
            // implement texturing
            if poly.pcw.texture() {
                // we need to perform a texture upload here and then set it as a TextureId on the PolygonDisplayItem
                let width = 8 << poly.type0.tsp.texture_u_size();
                let height = 8 << poly.type0.tsp.texture_v_size();

                let texture_addr = ((poly.type0.tcw.full & 0x1FFFFF) as usize) * 8;

                let bpp = match poly.type0.tcw.pixel_fmt() {
                    PvrPixelFmt::EightBpp => 8,
                    PvrPixelFmt::FourBpp => 4,
                    _ if poly.type0.tcw.texture_fmt() == PvrTextureFmt::Vq
                        || poly.type0.tcw.texture_fmt() == PvrTextureFmt::VqMipmaps =>
                    {
                        16 // 2 bytes per pixel, which is 16 bits
                    }
                    _ => 128, // 16 bytes per pixel, which is 128 bits
                };

                // convert bits per pixel to bytes per pixel
                let bytes_per_pixel = bpp / 8;

                // calculate the texture size
                let texture_size = (width * height * bytes_per_pixel) as usize;

                let texture = Texture {
                    width: width as u32,
                    height: height as u32,
                    addr: texture_addr as u32,
                    palette_addr: (poly.type0.tcw.palette.palette_selector() >> 4) << 10,
                };

                if let texture_id = TextureAtlas::hash_texture(
                    &texture,
                    poly.type0.tcw.texture_fmt(),
                    poly.type0.tcw.pixel_fmt(),
                ) {
                    self.pending_list.push(DisplayListItem::UploadTexture {
                        texture,
                        texture_format: poly.type0.tcw.texture_fmt(),
                        pixel_format: poly.type0.tcw.pixel_fmt(),
                    });

                    let face_colors = Self::parse_floating_color([
                        poly.type1.face_color_r,
                        poly.type1.face_color_g,
                        poly.type1.face_color_b,
                        poly.type1.face_color_a,
                    ]);

                    self.current_poly = Some(PolygonDisplayItem {
                        texture_id: Some(TextureId(texture_id)),
                        starting_vert: self.verts.len(),
                        tsp: poly.type0.tsp,
                        vert_len: 0,
                        poly_type: poly.pcw.poly_type(),
                        vert_type: poly.pcw.vert_type(),
                        face_color: face_colors,
                        palette_selector: (poly.type0.tcw.palette.palette_selector() >> 4) << 10,
                    });
                } else {
                    panic!("failed");
                }
            } else {
                //println!("begin poly with {}", self.verts.len());

                let face_colors = Self::parse_floating_color([
                    poly.type1.face_color_r,
                    poly.type1.face_color_g,
                    poly.type1.face_color_b,
                    poly.type1.face_color_a,
                ]);

                self.current_poly = Some(PolygonDisplayItem {
                    texture_id: None,
                    starting_vert: self.verts.len(),
                    tsp: poly.type0.tsp,
                    vert_len: 0,
                    vert_type: poly.pcw.vert_type(),
                    poly_type: poly.pcw.poly_type(),
                    face_color: face_colors,
                    palette_selector: (poly.type0.tcw.palette.palette_selector() >> 4) << 10,
                });
            }
        }
    }

    fn clamp(value: f32, min: f32, max: f32) -> f32 {
        if value < min {
            min
        } else if value > max {
            max
        } else {
            value
        }
    }

    fn ftou8(x: f32) -> u8 {
        let x = (x * 255.0).round() as i32;
        x.clamp(0, 255) as u8
    }

    fn parse_floating_color(color: [f32; 4]) -> [u8; 4] {
        let mut out = [0u8; 4];
        out[0] = Self::ftou8(color[0]);
        out[1] = Self::ftou8(color[1]);
        out[2] = Self::ftou8(color[2]);
        out[3] = Self::ftou8(color[3]);
        out
    }

    fn parse_intensity(color: [u8; 4], intensity: f32) -> [u8; 4] {
        let i = Self::ftou8(intensity);
        [
            Self::fmulu8(color[0], i),
            Self::fmulu8(color[1], i),
            Self::fmulu8(color[2], i),
            color[3],
        ]
    }

    pub fn push_vert(&mut self, value: VertexParam) {
        unsafe {
            let mut vert = VertexDefinition {
                x: 0.0,
                y: 0.0,
                z: 0.0,
                u: 0.0,
                v: 0.0,
                color: 0xff0000ff,
                end_of_strip: value.pcw.end_of_strip(),
            };

            if self.current_poly.is_none()
                && self.prev_poly.is_some()
                && self.prev_vert.is_some()
                && self.prev_vert.unwrap().pcw.end_of_strip()
            {
                self.current_poly = self.prev_poly;
                self.prev_poly = None;
            }

            if self.current_poly.is_none() {
                return;
            }

            let poly = self.current_poly.as_ref().unwrap();

            //println!("{:#?}", poly.vert_type);
            match poly.vert_type {
                VertexType::Type0 => {
                    let [x, y, z] = value.type0.xyz;
                    vert.x = x;
                    vert.y = y;
                    vert.z = z;

                    // from spec:
                    // Shading Color data (32-bit integers) for Packed Color format
                    // Store these values as is in the ISP/TSP Parameters
                    vert.color = value.type0.base_color;

                    self.verts.push(vert);
                    self.prev_vert = Some(value);
                }
                VertexType::Type1 => {
                    let [x, y, z] = value.type1.xyz;
                    vert.x = x;
                    vert.y = y;
                    vert.z = z;

                    // from the spec:
                    // Shading Color data (32-bit floating-point values) for Floating Color format
                    // Convert each data element into an 8-bit integer (0 to 255), group them into
                    // 32-bit values, and store them in the ISP/TSP Parameters.

                    let converted_color = [
                        (value.type1.base_color_r * 255.0) as u8,
                        (value.type1.base_color_g * 255.0) as u8,
                        (value.type1.base_color_b * 255.0) as u8,
                        (value.type1.base_color_a * 255.0) as u8,
                    ];

                    let color = (converted_color[0] as u32)
                        | (converted_color[1] as u32) << 8
                        | (converted_color[2] as u32) << 16
                        | (converted_color[3] as u32) << 24;
                    vert.color = color;

                    self.verts.push(vert);
                    self.prev_vert = Some(value);
                }
                VertexType::Type2 => {
                    let [x, y, z] = value.type2.xyz;
                    vert.x = x;
                    vert.y = y;
                    vert.z = z;

                    self.verts.push(vert);
                    self.prev_vert = Some(value);
                }
                VertexType::Type3 => {
                    let [x, y, z] = value.type3.xyz;
                    vert.x = x;
                    vert.y = y;
                    vert.z = z;

                    let [u, v] = value.type3.uv;

                    vert.u = v;
                    vert.v = u;
                    vert.color = value.type3.base_color;

                    self.verts.push(vert);
                    self.prev_vert = Some(value);
                }
                VertexType::Type4 => {
                    let [x, y, z] = value.type4.xyz;
                    vert.x = x;
                    vert.y = y;
                    vert.z = z;

                    let [v, u] = value.type4.uv;

                    vert.u = ((v as u32) << 16) as f32;
                    vert.v = ((u as u32) << 16) as f32;
                    vert.color = value.type4.base_color;

                    self.verts.push(vert);
                    self.prev_vert = Some(value);
                }
                VertexType::Type5 => {
                    let [x, y, z] = value.type5.xyz;
                    vert.x = x;
                    vert.y = y;
                    vert.z = z;

                    let [u, v] = value.type5.uv;

                    vert.u = v;
                    vert.v = u;

                    // fixme: base/offset intensity
                    self.verts.push(vert);
                    self.prev_vert = Some(value);
                }
                VertexType::Type7 => {
                    let [x, y, z] = value.type7.xyz;
                    vert.x = x;
                    vert.y = y;
                    vert.z = z;

                    let [u, v] = value.type7.uv;

                    vert.u = v;
                    vert.v = u;

                    let poly = self.current_poly.unwrap();
                    let converted_color =
                        Self::parse_intensity(poly.face_color, value.type7.base_intensity);

                    let color = (converted_color[0] as u32)
                        | (converted_color[1] as u32) << 8
                        | (converted_color[2] as u32) << 16
                        | (converted_color[3] as u32) << 24;
                    vert.color = color;

                    self.verts.push(vert);
                    self.prev_vert = Some(value);
                }
                VertexType::Type8 => {
                    let [x, y, z] = value.type8.xyz;
                    vert.x = x;
                    vert.y = y;
                    vert.z = z;

                    let [v, u] = value.type8.uv;

                    vert.u = ((v as u32) << 16) as f32;
                    vert.v = ((u as u32) << 16) as f32;

                    self.verts.push(vert);
                    self.prev_vert = Some(value);
                }
                VertexType::SpriteType0 | VertexType::SpriteType1 => {
                    assert!(value.pcw.end_of_strip());
                    self.commit_pending_polygon();

                    return;
                }
                VertexType::ModVol => {
                    return;
                }
                _ => {
                    let [x, y, z] = value.type0.xyz;
                    vert.x = x;
                    vert.y = y;
                    vert.z = z;

                    vert.color = value.type0.base_color;

                    self.verts.push(vert);
                    self.prev_vert = Some(value);
                }
            };

            if value.pcw.end_of_strip() {
                self.commit_pending_polygon();
            }
        }
    }

    fn calculate_average_depth(vertices: &[VertexDefinition]) -> f32 {
        (vertices[0].z + vertices[1].z + vertices[2].z) / 3.0
    }

    fn commit_pending_polygon(&mut self) {
        assert!(self.current_poly.is_some());

        let mut poly = self.current_poly.clone().unwrap();

        if !(self.verts.len() > poly.starting_vert) {
            self.current_poly = None;
            return;
        }

        poly.vert_len = self.verts.len() - poly.starting_vert;
        let poly_item = DisplayListItem::Polygon(poly);

        self.pending_list.push(poly_item);
        poly.vert_len = 0;
        poly.starting_vert = self.verts.len();

        self.prev_poly = Some(poly);
        self.current_poly = None;
    }

    pub fn build(&mut self) -> DisplayList {
        let items = mem::take(&mut self.pending_list);
        let verts = mem::take(&mut self.verts);

        assert!(self.pending_list.len() == 0);
        assert!(self.verts.len() == 0);
        self.prev_poly = None;
        self.current_poly = None;
        self.prev_vert = None;

        DisplayList { items, verts }
    }
}
