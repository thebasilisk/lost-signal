#include <metal_stdlib>

// xcrun -sdk macosx metal -o shaders.ir -c shaders.metal && xcrun -sdk macosx metallib -o shaders.metallib shaders.ir
// xcrun -sdk macosx metal -frecord-sources -gline-tables-only -c shaders.metal && xcrun -sdk macosx metal -frecord-sources -gline-tables-only -o shaders.metallib shaders.air

using namespace metal;

struct ColorInOut {
    float4 position [[ position ]];
    float4 color;
    float4 uv;
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
    out.uv = float4((float)(vid % 2), (float)((vid % 4) / 2), 0.0, 1.0);
    return out;
}


//float r1 = 0.5;
//float r2 = 0.3;
//float h = 1.0;
//float2 coords = float2(abs(in.uv.y - 0.5), in.uv.x - 0.5) * 4.0;
//float b = (r1 - r2) / h;
//float a = sqrt(1.0-b*b);
//float k = dot(coords, float2(-b,a));

//float d = 0.0;
//if( k < 0.0 ) {
//    d = length(coords) - r1;
//} else if( k > a*h ) {
//    d = length(coords-float2(0.0,h)) - r2;
//} else {
//    d = dot(coords, float2(a,b)) - r1;
//}

fragment float4 box_fragment (
    const device uniforms *unis,
    const device float2 *player_pos,
    const device float *signal_lost,
    ColorInOut in [[ stage_in ]]
) {
    //float2 coords = float2(in.uv.x * 8.0, in.uv.y);
    //float2 clamped_uv = float2(clamp(coords.x, 1.0, 7.0), 0.5);
    //float sdf_mask = -sign(distance(clamped_uv, coords) * 2.0 - 0.9);

    float screen_x = unis[0].screen_x;
    float screen_y = unis[0].screen_y;
    float radius = unis[0].radius;
    float2 pos_norm = float2((player_pos[0].x + screen_x) / 2.0, (-player_pos[0].y + screen_y) / 2.0);
    float4 grayscaled = float4(float3(in.color.r * 0.299 + 0.587 * in.color.g + in.color.b * 0.114), in.color.a);
    float t = saturate((distance(pos_norm, in.position.xy) / radius) + signal_lost[0]);
    float4 color_out = mix(in.color, grayscaled, t);
    //if (d > 0.0) discard_fragment();
    return color_out;
}

fragment float4 goal_fragment (
    const device float *t,
    ColorInOut in [[stage_in]]
) {
    float clamped_t = saturate(t[0]);
    float4 grayscaled = float4(float3(in.color.r * 0.299 + 0.587 * in.color.g + in.color.b * 0.114), 1.0);
    return mix(in.color, grayscaled, clamped_t);
}


fragment float4 target_fragment (
    const device uniforms *unis,
    const device float2 *player_pos,
    const device float *signal_lost,
    ColorInOut in [[ stage_in ]]
) {
    float screen_x = unis[0].screen_x;
    float screen_y = unis[0].screen_y;
    float radius = unis[0].radius;
    float2 pos_norm = float2((player_pos[0].x + screen_x) / 2.0, (-player_pos[0].y + screen_y) / 2.0);
    float4 grayscaled = float4(float3(in.color.r * 0.299 + 0.587 * in.color.g + in.color.b * 0.114), in.color.a);
    float t = saturate((distance(pos_norm, in.position.xy) / radius) + signal_lost[0]);
    float4 color_out = mix(in.color, grayscaled, t);

    float2 coords = float2(in.uv.x - 0.5, in.uv.y - 0.5);
    coords = abs(coords);
    float r = 0.06;
    float w = 0.4;

    float d = length(coords-min(coords.x+coords.y, w )*0.5) - r;
    if (d > 0) discard_fragment();

    return color_out;
}

fragment float4 scorezone_fragment (
    const device float *t,
    ColorInOut in [[ stage_in ]]
) {
    float2 coords0 = float2(in.uv.x - 0.5, in.uv.y - 0.5);
    //float rdius = coords

    float2 coords = float2(in.uv.x - 0.5, in.uv.y - 0.5) * 2.0;
    float he = 1.2;
    float ra = 0.2;

    coords = abs(coords);
    coords = float2(abs(coords.x-coords.y),1.0-coords.x-coords.y)/sqrt(2.0);

    float p = (he-coords.y-0.25/he)/(6.0*he);
    float q = coords.x/(he*he*16.0);
    float h = q*q - p*p*p;

    float x;
    if( h>0.0 ) { float r = sqrt(h); x = pow(q+r,1.0/3.0)-pow(abs(q-r),1.0/3.0)*sign(r-q); }
    else        { float r = sqrt(p); x = 2.0*r*cos(acos(q/(p*r))/3.0); }
    x = min(x,sqrt(2.0)/2.0);

    float2 z = float2(x,he*(1.0-2.0*x*x)) - coords;
    float inner_d = (length(z) * sign(z.y)) - ra;

    float2 coords1 = coords0 * 0.9;
    float2 b = float2(0.06125, 0.06125);
    float2 a = abs(coords1)-b;
    float d = length(max(a,0.0)) + min(max(a.x,a.y),0.0);
    //float annular_rect = sign(abs(d) - 0.2);

    if ((sign(inner_d) == 1) && sign(d) == 1) discard_fragment();

    float clamped_t = saturate(t[0]);
    float4 grayscaled = float4(float3(in.color.r * 0.299 + 0.587 * in.color.g + in.color.b * 0.114), 0.0);
    return mix(in.color, grayscaled, clamped_t);
}
