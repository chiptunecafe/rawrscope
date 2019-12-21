#version 450

layout(location = 0) in vec2 f_TexCoord;
layout(location = 0) out vec4 f_Color;

layout(set = 0, binding = 1) uniform texture2D u_Tex;
layout(set = 0, binding = 2) uniform sampler u_Sampler;

void main() {
    f_Color = texture(sampler2D(u_Tex, u_Sampler), f_TexCoord);
}
