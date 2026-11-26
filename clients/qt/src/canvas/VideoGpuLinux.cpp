#include "canvas/VideoDecoder.hpp"
#include "canvas/VideoNode.hpp"

#include <QOpenGLContext>
#include <QOpenGLFunctions>
#include <rhi/qrhi.h>

#include <EGL/egl.h>
#include <EGL/eglext.h>
#include <va/va.h>

extern "C" {
#include <libavutil/frame.h>
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_drm.h>
#include <libavutil/hwcontext_vaapi.h>
#include <libavutil/pixfmt.h>
}

#include <algorithm>
#include <array>
#include <cstring>
#include <optional>
#include <string_view>
#include <utility>
#include <vector>

#ifndef EGL_DRM_RENDER_NODE_FILE_EXT
#define EGL_DRM_RENDER_NODE_FILE_EXT 0x3377
#endif

namespace kodosi {
namespace {
using TargetTexture = void (*)(GLenum target, void* image);

constexpr EGLint fourcc(char a, char b, char c, char d)
{
    return static_cast<EGLint>(static_cast<uint32_t>(a) | static_cast<uint32_t>(b) << 8U
        | static_cast<uint32_t>(c) << 16U | static_cast<uint32_t>(d) << 24U);
}

// The DRM formats of NV12's planes, from drm_fourcc.h.
constexpr std::array planeFormats { fourcc('R', '8', ' ', ' '), fourcc('G', 'R', '8', '8') };
constexpr uint64_t invalidModifier = 0x00ffffffffffffffULL;

bool hasExtension(const char* extensions, std::string_view name)
{
    std::string_view rest = extensions != nullptr ? extensions : "";
    while (!rest.empty()) {
        const auto end = rest.find(' ');
        if (rest.substr(0, end) == name)
            return true;
        if (end == std::string_view::npos)
            break;
        rest.remove_prefix(end + 1);
    }
    return false;
}

// The render node of the GPU that Qt Quick draws with. Nothing when Qt cannot take a DMA-BUF, and empty
// when EGL does not name the GPU.
std::optional<QByteArray> qtRenderNode()
{
    QOpenGLContext context;
    if (!context.create())
        return std::nullopt;
    const auto* egl = context.nativeInterface<QNativeInterface::QEGLContext>();
    if (egl == nullptr)
        return std::nullopt;
    const auto display = egl->display();
    if (!hasExtension(eglQueryString(display, EGL_EXTENSIONS), "EGL_EXT_image_dma_buf_import"))
        return std::nullopt;
    const auto queryDisplay
        = reinterpret_cast<PFNEGLQUERYDISPLAYATTRIBEXTPROC>(eglGetProcAddress("eglQueryDisplayAttribEXT"));
    const auto queryDevice
        = reinterpret_cast<PFNEGLQUERYDEVICESTRINGEXTPROC>(eglGetProcAddress("eglQueryDeviceStringEXT"));
    EGLAttrib device = 0;
    if (queryDisplay == nullptr || queryDevice == nullptr || queryDisplay(display, EGL_DEVICE_EXT, &device) == EGL_FALSE)
        return QByteArray();
    const char* node = queryDevice(reinterpret_cast<EGLDeviceEXT>(device), EGL_DRM_RENDER_NODE_FILE_EXT);
    return node != nullptr ? QByteArray(node) : QByteArray();
}

bool decodes(VADisplay display, VAProfile profile)
{
    std::vector<VAEntrypoint> entrypoints(static_cast<size_t>(std::max(vaMaxNumEntrypoints(display), 0)));
    int count = 0;
    if (vaQueryConfigEntrypoints(display, profile, entrypoints.data(), &count) != VA_STATUS_SUCCESS)
        return false;
    const auto listed = entrypoints.begin() + std::clamp(count, 0, static_cast<int>(entrypoints.size()));
    return std::find(entrypoints.begin(), listed, VAEntrypointVLD) != listed;
}

QSize planeSize(QSize size, size_t plane)
{
    return plane == 0 ? size : QSize((size.width() + 1) / 2, (size.height() + 1) / 2);
}

// The VA-API decoder's surfaces, which OpenGL samples in place through DMA-BUF.
class VaapiPlanes final : public VideoPlanes {
public:
    VaapiPlanes(QOpenGLContext& context, EGLDisplay display)
        : m_context(&context)
        , m_display(display)
        , m_createImage(reinterpret_cast<PFNEGLCREATEIMAGEKHRPROC>(eglGetProcAddress("eglCreateImageKHR")))
        , m_destroyImage(reinterpret_cast<PFNEGLDESTROYIMAGEKHRPROC>(eglGetProcAddress("eglDestroyImageKHR")))
        , m_targetTexture(reinterpret_cast<TargetTexture>(context.getProcAddress("glEGLImageTargetTexture2DOES")))
        , m_modifiers(hasExtension(eglQueryString(display, EGL_EXTENSIONS), "EGL_EXT_image_dma_buf_import_modifiers"))
    {
        context.functions()->glGenTextures(static_cast<GLsizei>(m_textures.size()), m_textures.data());
    }

    ~VaapiPlanes() override
    {
        m_planes = {};
        if (QOpenGLContext::currentContext() == m_context)
            m_context->functions()->glDeleteTextures(static_cast<GLsizei>(m_textures.size()), m_textures.data());
    }

    VaapiPlanes(const VaapiPlanes&) = delete;
    VaapiPlanes& operator=(const VaapiPlanes&) = delete;

    [[nodiscard]] bool usable() const
    {
        return m_createImage != nullptr && m_destroyImage != nullptr && m_targetTexture != nullptr;
    }

    bool take(QRhi& rhi, QRhiResourceUpdateBatch&, std::shared_ptr<const VideoFrame> frame) override
    {
        auto* mapped = av_frame_alloc();
        if (mapped == nullptr)
            return false;
        mapped->format = AV_PIX_FMT_DRM_PRIME;
        bool bound = av_hwframe_map(mapped, frame->av(), AV_HWFRAME_MAP_READ) >= 0;
        const auto size = frame->size();
        for (size_t plane = 0; bound && plane < m_textures.size(); ++plane)
            bound = bind(*reinterpret_cast<const AVDRMFrameDescriptor*>(mapped->data[0]), plane, planeSize(size, plane));
        av_frame_free(&mapped);
        if (!bound || !fit(rhi, size))
            return false;
        // OpenGL reads the decoder's surface in place, so the surface stays out of the decoder's pool until
        // the picture after the next one is on screen.
        m_before = std::exchange(m_frame, std::move(frame));
        return true;
    }

    [[nodiscard]] QRhiTexture* luma() const override { return m_planes[0].get(); }
    [[nodiscard]] QRhiTexture* chroma() const override { return m_planes[1].get(); }

private:
    // Points the plane's texture at its part of the surface. A surface exported layer by layer has a layer
    // for each plane; otherwise its one layer holds both.
    bool bind(const AVDRMFrameDescriptor& surface, size_t plane, QSize size)
    {
        const bool layered = surface.nb_layers >= 2;
        if (!layered && (surface.nb_layers != 1 || surface.layers[0].nb_planes < 2))
            return false;
        const auto& layer = surface.layers[layered ? plane : 0];
        const auto& part = layer.planes[layered ? 0 : plane];
        const auto& object = surface.objects[part.object_index];
        std::vector<EGLint> attributes { EGL_WIDTH, size.width(), EGL_HEIGHT, size.height(), EGL_LINUX_DRM_FOURCC_EXT,
            layered ? static_cast<EGLint>(layer.format) : planeFormats.at(plane), EGL_DMA_BUF_PLANE0_FD_EXT, object.fd,
            EGL_DMA_BUF_PLANE0_OFFSET_EXT, static_cast<EGLint>(part.offset), EGL_DMA_BUF_PLANE0_PITCH_EXT,
            static_cast<EGLint>(part.pitch) };
        if (m_modifiers && object.format_modifier != invalidModifier) {
            attributes.insert(attributes.end(),
                { EGL_DMA_BUF_PLANE0_MODIFIER_LO_EXT, static_cast<EGLint>(object.format_modifier & 0xffffffffU),
                    EGL_DMA_BUF_PLANE0_MODIFIER_HI_EXT, static_cast<EGLint>(object.format_modifier >> 32U) });
        }
        attributes.push_back(EGL_NONE);
        const auto image = m_createImage(m_display, EGL_NO_CONTEXT, EGL_LINUX_DMA_BUF_EXT, nullptr, attributes.data());
        if (image == EGL_NO_IMAGE_KHR)
            return false;
        auto* gl = m_context->functions();
        GLint previous = 0;
        gl->glGetIntegerv(GL_TEXTURE_BINDING_2D, &previous);
        gl->glBindTexture(GL_TEXTURE_2D, m_textures.at(plane));
        m_targetTexture(GL_TEXTURE_2D, image);
        gl->glBindTexture(GL_TEXTURE_2D, static_cast<GLuint>(previous));
        // The texture keeps the buffer that the image named.
        m_destroyImage(m_display, image);
        return true;
    }

    bool fit(QRhi& rhi, QSize size)
    {
        if (m_planes[0] && m_planes[0]->pixelSize() == size)
            return true;
        for (size_t plane = 0; plane < m_planes.size(); ++plane) {
            m_planes.at(plane).reset(
                rhi.newTexture(plane == 0 ? QRhiTexture::R8 : QRhiTexture::RG8, planeSize(size, plane)));
            if (!m_planes.at(plane)->createFrom({ m_textures.at(plane), 0 })) {
                m_planes = {};
                return false;
            }
        }
        return true;
    }

    QOpenGLContext* m_context;
    EGLDisplay m_display;
    PFNEGLCREATEIMAGEKHRPROC m_createImage;
    PFNEGLDESTROYIMAGEKHRPROC m_destroyImage;
    TargetTexture m_targetTexture;
    bool m_modifiers;
    std::array<GLuint, 2> m_textures {};
    std::array<std::unique_ptr<QRhiTexture>, 2> m_planes;
    std::shared_ptr<const VideoFrame> m_frame;
    std::shared_ptr<const VideoFrame> m_before;
};
}

std::shared_ptr<VideoGpu> findVideoGpu()
{
    const auto node = qtRenderNode();
    if (!node)
        return nullptr;
    AVBufferRef* device = nullptr;
    if (av_hwdevice_ctx_create(&device, AV_HWDEVICE_TYPE_VAAPI, node->isEmpty() ? nullptr : node->constData(), nullptr, 0)
        < 0)
        return nullptr;
    auto gpu = std::make_shared<VideoGpu>();
    gpu->device = device;
    const auto* context = reinterpret_cast<const AVHWDeviceContext*>(device->data);
    const auto display = static_cast<const AVVAAPIDeviceContext*>(context->hwctx)->display;
    gpu->h264 = decodes(display, VAProfileH264High);
    gpu->hevc = decodes(display, VAProfileHEVCMain);
    return gpu->h264 || gpu->hevc ? gpu : nullptr;
}

int gpuPixelFormat()
{
    return AV_PIX_FMT_VAAPI;
}

std::unique_ptr<VideoPlanes> gpuPlanes(QRhi& rhi)
{
    auto* context = QOpenGLContext::currentContext();
    const auto* egl = context != nullptr ? context->nativeInterface<QNativeInterface::QEGLContext>() : nullptr;
    if (rhi.backend() != QRhi::OpenGLES2 || egl == nullptr)
        return nullptr;
    auto planes = std::make_unique<VaapiPlanes>(*context, egl->display());
    return planes->usable() ? std::move(planes) : nullptr;
}
}
