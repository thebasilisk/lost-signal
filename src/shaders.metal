#include <metal_stdlib>

// xcrun -sdk macosx metal -o shaders.ir -c shaders.metal && xcrun -sdk macosx metallib -o shaders.metallib shaders.ir

using namespace metal;

struct ColorInOut {
    float4 position [[ position ]];
    float4 color;
};

struct vertex_t {
    float4 pos;
    float4 col;
};

struct uniforms {
    float screen_x;
    float screen_y;
    float radius;
};

vertex ColorInOut box_vertex (
    const device uniforms *unis,
    const device vertex_t *verts,
    uint vid [[ vertex_id ]]
) {
    ColorInOut out;

    uniforms uni = unis[0];
    float screen_x = uni.screen_x;
    float screen_y = uni.screen_y;

    out.position = float4(verts[vid].pos.x / screen_x, verts[vid].pos.y / screen_y, 0.0, 1.0);
    out.color = verts[vid].col;

    return out;
}


fragment float4 box_fragment (
    const device uniforms *unis,
    const device float2 *player_pos,
    ColorInOut in [[ stage_in ]]
) {
    float screen_x = unis[0].screen_x;
    float screen_y = unis[0].screen_y;
    float radius = unis[0].radius;
    float2 pos_norm = float2((player_pos[0].x + screen_x) / 2.0, (-player_pos[0].y + screen_y) / 2.0);
    float4 grayscaled = float4(float3(in.color.r * 0.299 + 0.587 * in.color.g + in.color.b * 0.114), 1.0);
    float t = saturate(distance(pos_norm, in.position.xy) / radius);
    float4 color_out = mix(in.color, grayscaled, t);
    return color_out;
}

fragment float4 goal_fragment (
    ColorInOut in [[stage_in]]
) {
    return in.color;
}
