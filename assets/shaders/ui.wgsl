struct UIVertex {
    @location(0) position: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct UIVertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

struct Uniforms {
    screen_size: vec2<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var texture_sampler: texture_2d<f32>;

@group(0) @binding(2)
var sampler_ref: sampler;

@vertex
fn vs_main(vertex: UIVertex) -> UIVertexOutput {
    var output: UIVertexOutput;
    let normalized = vertex.position / uniforms.screen_size;
    output.clip_position = vec4(
        normalized.x * 2.0 - 1.0,
        -(normalized.y * 2.0 - 1.0),
        0.0,
        1.0
    );
    output.uv = vertex.uv;
    output.color = vertex.color;
    return output;
}

@fragment
fn fs_main(input: UIVertexOutput) -> @location(0) vec4<f32> {
    let tex_color = textureSample(texture_sampler, sampler_ref, input.uv);
    return tex_color * input.color;
}
