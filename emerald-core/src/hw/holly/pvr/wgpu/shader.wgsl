struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) @interpolate(flat) array_index: u32,
    @location(4) @interpolate(flat) texture_id: u32,
    @location(5) @interpolate(flat) textured: u32,
    @location(6) @interpolate(flat) ignore_alpha: u32,
    @location(7) @interpolate(flat) shading: u32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) array_index: u32,
    @location(3) @interpolate(flat) texture_id: u32,
    @location(4) @interpolate(flat) textured: u32,
    @location(5) @interpolate(flat) ignore_alpha: u32,
    @location(6) @interpolate(flat) shading: u32,
};

@group(0) @binding(0) var texture_array_8x8: texture_2d_array<f32>;
@group(0) @binding(1) var texture_array_16x16: texture_2d_array<f32>;
@group(0) @binding(2) var texture_array_32x32: texture_2d_array<f32>;
@group(0) @binding(3) var texture_array_64x64: texture_2d_array<f32>;
@group(0) @binding(4) var texture_array_128x128: texture_2d_array<f32>;
@group(0) @binding(5) var texture_array_256x256: texture_2d_array<f32>;
@group(0) @binding(6) var texture_array_512x512: texture_2d_array<f32>;
@group(0) @binding(7) var texture_array_1024x1024: texture_2d_array<f32>;
@group(0) @binding(8) var image_sampler: sampler;

fn tex_sample(color: vec4<f32>, uv: vec2<f32>, array_index: u32, index: u32) -> vec4<f32> {
    let tex_size = tex_size(array_index);
    let ddx = dpdx(uv);
    let ddy = dpdy(uv);

    switch(array_index)  {
        case 0u: { return textureSampleGrad(texture_array_8x8, image_sampler, uv, index, ddx, ddy); }
        case 1u: { return textureSampleGrad(texture_array_16x16, image_sampler, uv, index, ddx, ddy); }
        case 2u: { return textureSampleGrad(texture_array_32x32, image_sampler, uv, index, ddx, ddy); }
        case 3u: { return textureSampleGrad(texture_array_64x64, image_sampler, uv, index, ddx, ddy); }
        case 4u: { return textureSampleGrad(texture_array_128x128, image_sampler, uv, index, ddx, ddy); }
        case 5u: { return textureSampleGrad(texture_array_256x256, image_sampler, uv, index, ddx, ddy); }
        case 6u: { return textureSampleGrad(texture_array_512x512, image_sampler, uv, index, ddx, ddy); }
        case 7u: { return textureSampleGrad(texture_array_1024x1024, image_sampler, uv, index, ddx, ddy); }
        default: { return vec4<f32>(1.0, 0.0, 0.0, 1.0); }
    }
}


fn tex_size(idx: u32) -> f32 {
    switch(idx)  {
        case 0u: { return 8.0; }
        case 1u: { return 16.0; }
        case 2u: { return 32.0; }
        case 3u: { return 64.0; }
        case 4u: { return 128.0; }
        case 5u: { return 256.0; }
        case 6u: { return 512.0; }
        case 7u: { return 1024.0; }
        default: { return 8.0; }
    }
}


@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.color = model.color;
    out.uv = model.uv;
    out.array_index = model.array_index;
    out.texture_id = model.texture_id;

    // fixme: stop hardcoding this
    let screen_size = vec2<f32>(640.0, 480.0);
    out.clip_position.x = model.position.x * 2.0 / screen_size.x - 1.0;
    out.clip_position.y = model.position.y * -2.0 / screen_size.y + 1.0;

    let near_plane = 0.1;
    let far_plane = 10000.0;
    out.clip_position.z = (model.position.z - near_plane) / (far_plane - near_plane);
    out.clip_position.w = 1.0;
    out.textured = model.textured;
    out.ignore_alpha = model.ignore_alpha;
    out.shading = model.shading;

    return out;}


@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if (in.textured == 0) {
        return in.color;
    }

    let tex_color = tex_sample(in.color, in.uv, in.array_index, in.texture_id);
    let shading = in.shading;
    let ignore_tex_alpha = f32(in.ignore_alpha);
    let tex_a = select(tex_color.a, 1.0, ignore_tex_alpha > 0.0);
    var final_color = in.color;
    let base_color = in.color;
    let offset_color = vec4<f32>(0.0, 0.0, 0.0, 0.0);

    switch(shading)  {
        // decal
        case 0u: {
            let rgb = tex_color.rgb + offset_color.rgb;
            let a = tex_a;
            final_color = vec4<f32>(rgb, a);
        }
        // modulate
        case 1u: {
            let rgb = base_color.rgb * tex_color.rgb + offset_color.rgb;
            let a = tex_a;
            final_color = vec4<f32>(rgb, a);
        }
        // decal + alpha
        case 2u: {
            let rgb = tex_a * tex_color.rgb + (1.0 - tex_a) * base_color.rgb + offset_color.rgb;
            let a = base_color.a;
            final_color = vec4<f32>(rgb, a);
        }
        // modulate + alpha
        case 3u: {
            let rgb = base_color.rgb * tex_color.rgb + offset_color.rgb;
            let a = base_color.a * tex_a;
            final_color = vec4<f32>(rgb, a);
        }
        default: { final_color = base_color + vec4<f32>(offset_color.rgb, 0.0); }
    }

    if final_color.a == 0 {
        discard;
    }

    return final_color;
}
