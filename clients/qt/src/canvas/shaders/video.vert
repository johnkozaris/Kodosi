#version 440

layout(location = 0) in vec4 position;
layout(location = 1) in vec2 corner;

layout(location = 0) out vec2 at;

layout(std140, binding = 0) uniform Placement {
    mat4 matrix;
    float opacity;
};

void main()
{
    at = corner;
    gl_Position = matrix * position;
}
