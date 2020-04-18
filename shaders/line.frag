#version 450

layout(location = 0) in vec4 f_Endpoints;
layout(location = 0) out vec4 f_Color;

layout(std140, set = 1, binding = 0) uniform Uniforms {
    vec4 u_Resolution;
    mat4 u_Transform;
    float u_Thickness;
    int u_BaseIndex;
};

float segmentDistance(vec2 v, vec2 w, vec2 p) {
    float l2 = length(w - v);
    l2 *= l2;
    float t = clamp(dot(p - v, w - v) / l2, 0.0, 1.0);
    vec2 projection = v + t * (w - v);
    return distance(p, projection);
}

void main() {
    // FIXME weird hack around new wgpu screen coordinates, may be totally wrong approach
    vec2 fixed_coords = vec2(gl_FragCoord.x, u_Resolution.y - gl_FragCoord.y);
    float dist = segmentDistance(f_Endpoints.xy, f_Endpoints.zw, fixed_coords) - u_Thickness / 2.0;

    f_Color = vec4(vec3(1), clamp(0.5 - dist, 0.0, 1.0));
}
