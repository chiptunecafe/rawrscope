#version 450

// 2 points packed into one vec4
layout(location = 0) out vec4 f_Endpoints;

layout(std430, set = 0, binding = 0) buffer DataBuffer {
    float sb_LineData[];
};

layout(std140, set = 1, binding = 0) uniform Uniforms {
    vec4 u_Resolution;
    mat4 u_Transform;
    float u_Thickness;
    int u_BaseIndex;
};

const int c_DirLut[6] = int[6](1, 1, -1, -1, -1, 1);

void main() {
    // fetch and transform line endpoints
    int line_idx = (gl_VertexIndex) / 6;
    vec2 a = (u_Transform * vec4(line_idx, sb_LineData[line_idx + u_BaseIndex], 0.0, 1.0)).xy;
    vec2 b = (u_Transform * vec4(line_idx + 1, sb_LineData[line_idx + u_BaseIndex + 1], 0.0, 1.0)).xy;

    // write endpoints
    f_Endpoints = vec4((a * 0.5 + 0.5) * u_Resolution.xy, (b * 0.5 + 0.5) * u_Resolution.xy);

    // transform thickness to normalized screen coordinates and add feathering distance
    vec2 thickness_norm = vec2(u_Thickness + 1.0) / u_Resolution.xy;

    // find vector perpendicular to line
    vec2 m = normalize(b - a);
    vec2 n = vec2(-m.y, m.x);

    // extend endpoints
    a -= thickness_norm * m;
    b += thickness_norm * m;

    // write vert (crazy branchless logic :))
    int quad_idx = gl_VertexIndex % 6;
    gl_Position = vec4(mix(a, b, quad_idx % 2) + n * thickness_norm * c_DirLut[quad_idx], 0.0, 1.0);
}
