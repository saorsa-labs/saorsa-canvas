// Textured quad shader for rendering images and chart textures

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct Uniforms {
    // Transform: x, y, width, height
    transform: vec4<f32>,
    // Canvas dimensions: width, height, reserved, reserved
    canvas_size: vec4<f32>,
    // Tint color (multiplied with texture)
    tint: vec4<f32>,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var t_diffuse: texture_2d<f32>;

@group(0) @binding(2)
var s_diffuse: sampler;

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    // Scale vertex position by element size
    let scaled_pos = in.position * uniforms.transform.zw;

    // Translate to element position
    let world_pos = scaled_pos + uniforms.transform.xy;

    // Convert to normalized device coordinates (-1 to 1)
    let ndc_x = (world_pos.x / uniforms.canvas_size.x) * 2.0 - 1.0;
    let ndc_y = 1.0 - (world_pos.y / uniforms.canvas_size.y) * 2.0;

    out.clip_position = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = in.uv;

    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(t_diffuse, s_diffuse, in.uv);
    return tex_color * uniforms.tint;
}
