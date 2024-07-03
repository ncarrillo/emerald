// Vertex shader

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@group(0) @binding(0) var fb_texture: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;

@vertex
fn vs_main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.uv = model.uv;
    out.clip_position = vec4<f32>(model.position.xy, 0.0, 1.0); // Ensure depth is within the range [0, 1]
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(fb_texture, image_sampler, in.uv);
}
