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

vertex ColorInOut box_vertex (
    const device float4 *uniforms,
    const device vertex_t *verts,
    uint vid [[ vertex_id ]]
) {
    ColorInOut out;

    float2 screen_size = uniforms[0].xy;
    out.position = float4(verts[vid].pos.x / screen_size.x, verts[vid].pos.y / screen_size.y, 0.0, 1.0);
    float4 color = verts[vid].col;
    out.color = float4(color.r * 0.299 + 0.587 * color.g + color.b * 0.114);
    out.color.a = 1.0;

    return out;
}


fragment float4 box_fragment (
    ColorInOut in [[ stage_in ]]
) {
    return in.color;
}
