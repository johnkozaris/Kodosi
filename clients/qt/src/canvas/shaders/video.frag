#version 440

layout(location = 0) in vec2 at;

layout(location = 0) out vec4 colour;

layout(std140, binding = 0) uniform Placement {
    mat4 matrix;
    float opacity;
};

layout(binding = 1) uniform sampler2D luma;
layout(binding = 2) uniform sampler2D chroma;

// BT.709 limited range, as every Kodosi encoder writes it.
void main()
{
    float y = (texture(luma, at).r - 16.0 / 255.0) * (255.0 / 219.0);
    vec2 c = (texture(chroma, at).rg - 128.0 / 255.0) * (255.0 / 224.0);
    colour = vec4(y + 1.5748 * c.y, y - 0.1873 * c.x - 0.4681 * c.y, y + 1.8556 * c.x, 1.0) * opacity;
}
