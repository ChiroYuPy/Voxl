struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) voxel_pos: vec3<i32>,
}

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
}

struct Camera {
    view_proj: mat4x4<f32>,
}
@group(0) @binding(0) var<uniform> camera: Camera;

struct TimeUniforms {
    time: f32,
}
@group(1) @binding(0) var<uniform> time_data: TimeUniforms;

@vertex
fn vs_main(vertex: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let world_pos = vec3<f32>(vertex.voxel_pos) + vertex.position;
    out.clip_position = camera.view_proj * vec4<f32>(world_pos, 1.0);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let pulse_speed = 3.14159 * 2.0;
    let sine_value = sin(time_data.time * pulse_speed);
    let alpha = 0.2 + (sine_value + 1.0) * 0.2;

    // Semi-transparent black
    return vec4<f32>(0.0, 0.0, 0.0, alpha);
}
