#version 440

layout(location = 0) in vec2 at;

layout(location = 0) out vec4 colour;

layout(std140, binding = 0) uniform Placement {
    mat4 matrix;
    float opacity;
    float rest;
};

layout(binding = 1) uniform sampler2D luma;
layout(binding = 2) uniform sampler2D chroma;

// BT.709 limited range, as every Kodosi encoder writes it. A resting picture shows at half its
// brightness and with a little over half its colour.
void main()
{
    float y = (texture(luma, at).r - 16.0 / 255.0) * (255.0 / 219.0);
    vec2 c = (texture(chroma, at).rg - 128.0 / 255.0) * (255.0 / 224.0);
    vec3 rgb = vec3(y + 1.5748 * c.y, y - 0.1873 * c.x - 0.4681 * c.y, y + 1.8556 * c.x);
    vec3 resting = mix(vec3(y), rgb, 0.55) * 0.5;
    colour = vec4(mix(rgb, resting, rest), 1.0) * opacity;
}
