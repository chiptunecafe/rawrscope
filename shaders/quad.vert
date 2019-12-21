#version 450

layout(location = 0) in vec3 v_Pos;
layout(location = 1) in vec2 v_TexCoord;

layout(location = 0) out vec2 f_TexCoord;

layout(set = 0, binding = 0) uniform Uniforms {
    mat4 u_Transform;
};

void main() {
    f_TexCoord = v_TexCoord;
    gl_Position = u_Transform * vec4(v_Pos, 1.0);
}
