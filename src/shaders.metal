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
    uint last_vert;
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
    uint last_vert = uni.last_vert;

    out.position = float4(verts[vid].pos.x / screen_x, verts[vid].pos.y / screen_y, 0.0, 1.0);
    float4 color = verts[vid].col;
    if (vid < last_vert) {
        out.color = float4(color.r * 0.299 + 0.587 * color.g + color.b * 0.114);
    } else {
        out.color = color;
    }
    out.color.a = 1.0;

    return out;
}


fragment float4 box_fragment (
    ColorInOut in [[ stage_in ]]
) {
    return in.color;
}
