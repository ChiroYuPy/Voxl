struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) voxel_pos: vec3<i32>,
    @location(3) uv: vec2<f32>,
    @location(4) color: vec4<f32>,
}

struct Camera {
    view_proj: mat4x4<f32>,
}
@group(0) @binding(0) var<uniform> camera: Camera;

@vertex
fn vs_main(vertex: VertexInput) -> FragmentInput {
    var out: FragmentInput;

    let world_pos = vec3<f32>(vertex.voxel_pos) + vertex.position;

    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    out.uv = vertex.uv;
    out.color = vertex.color;

    return out;
}

// fragment
struct FragmentInput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>, // precalculated color (light * AO)
}

@group(0) @binding(1) var my_texture: texture_2d<f32>;
@group(0) @binding(2) var my_sampler: sampler;

@fragment
fn fs_main(in: FragmentInput) -> @location(0) vec4<f32> {
    let texture_color = textureSample(my_texture, my_sampler, in.uv).rgb;

    return vec4<f32>(texture_color * in.color.rgb, 1.0);
}
