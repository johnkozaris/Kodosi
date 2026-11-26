#include "canvas/VideoNode.hpp"
#include "canvas/VideoDecoder.hpp"

#include <QSGMaterial>
#include <QSGMaterialShader>
#include <QSGTexture>
#include <rhi/qrhi.h>

extern "C" {
#include <libavutil/frame.h>
#include <libavutil/pixfmt.h>
}

#include <cstring>
#include <functional>
#include <utility>

namespace kodosi {
namespace {
constexpr int lumaBinding = 1;
constexpr qsizetype matrixBytes = 64;
constexpr char blackLuma = 16;
constexpr char neutralChroma = static_cast<char>(128);

QSize chromaSize(QSize size)
{
    return { (size.width() + 1) / 2, (size.height() + 1) / 2 };
}

void upload(QRhiResourceUpdateBatch& updates, QRhiTexture* texture, const QByteArray& plane, int stride)
{
    QRhiTextureSubresourceUploadDescription description(plane);
    description.setDataStride(static_cast<quint32>(stride));
    updates.uploadTexture(texture, QRhiTextureUploadEntry(0, 0, description));
}

// A plane that the scene graph samples, held by the planes of the picture on screen.
class PlaneTexture final : public QSGTexture {
public:
    PlaneTexture()
    {
        setFiltering(Linear);
        setHorizontalWrapMode(ClampToEdge);
        setVerticalWrapMode(ClampToEdge);
    }

    void hold(QRhiTexture* texture) { m_texture = texture; }
    [[nodiscard]] qint64 comparisonKey() const override
    {
        return static_cast<qint64>(reinterpret_cast<quintptr>(m_texture));
    }
    [[nodiscard]] QRhiTexture* rhiTexture() const override { return m_texture; }
    [[nodiscard]] QSize textureSize() const override
    {
        return m_texture != nullptr ? m_texture->pixelSize() : QSize();
    }
    [[nodiscard]] bool hasAlphaChannel() const override { return false; }
    [[nodiscard]] bool hasMipmaps() const override { return false; }

private:
    QRhiTexture* m_texture = nullptr;
};

// Planes that the processor uploads: the software decoder's pictures, or black while nothing else shows.
class MemoryPlanes final : public VideoPlanes {
public:
    bool take(QRhi& rhi, QRhiResourceUpdateBatch& updates, std::shared_ptr<const VideoFrame> frame) override
    {
        const auto* av = frame->av();
        if (av->format != AV_PIX_FMT_YUV420P && av->format != AV_PIX_FMT_YUVJ420P)
            return false;
        const auto size = frame->size();
        if (!fit(rhi, size))
            return false;
        // The upload reads the frame's own memory, which this keeps until the next picture.
        upload(updates, m_luma.get(),
            QByteArray::fromRawData(reinterpret_cast<const char*>(av->data[0]), qsizetype(av->linesize[0]) * size.height()),
            av->linesize[0]);
        const auto half = chromaSize(size);
        m_chromaPlane.resize(qsizetype(half.width()) * 2 * half.height());
        auto* interleaved = reinterpret_cast<uint8_t*>(m_chromaPlane.data());
        for (int row = 0; row < half.height(); ++row) {
            const auto* blue = av->data[1] + qsizetype(row) * av->linesize[1];
            const auto* red = av->data[2] + qsizetype(row) * av->linesize[2];
            auto* line = interleaved + qsizetype(row) * half.width() * 2;
            for (int column = 0; column < half.width(); ++column) {
                line[2 * column] = blue[column];
                line[2 * column + 1] = red[column];
            }
        }
        upload(updates, m_chroma.get(), m_chromaPlane, half.width() * 2);
        m_frame = std::move(frame);
        return true;
    }

    bool black(QRhi& rhi, QRhiResourceUpdateBatch& updates)
    {
        if (!fit(rhi, QSize(2, 2)))
            return false;
        upload(updates, m_luma.get(), QByteArray(4, blackLuma), 2);
        upload(updates, m_chroma.get(), QByteArray(2, neutralChroma), 2);
        return true;
    }

    [[nodiscard]] QRhiTexture* luma() const override { return m_luma.get(); }
    [[nodiscard]] QRhiTexture* chroma() const override { return m_chroma.get(); }

private:
    bool fit(QRhi& rhi, QSize size)
    {
        if (m_luma && m_luma->pixelSize() == size)
            return true;
        m_luma.reset(rhi.newTexture(QRhiTexture::R8, size));
        m_chroma.reset(rhi.newTexture(QRhiTexture::RG8, chromaSize(size)));
        if (m_luma->create() && m_chroma->create())
            return true;
        m_luma.reset();
        m_chroma.reset();
        return false;
    }

    std::unique_ptr<QRhiTexture> m_luma;
    std::unique_ptr<QRhiTexture> m_chroma;
    QByteArray m_chromaPlane;
    std::shared_ptr<const VideoFrame> m_frame;
};

class VideoShader;

class VideoMaterial final : public QSGMaterial {
public:
    VideoMaterial() { setFlag(Blending); }

    [[nodiscard]] QSGMaterialType* type() const override
    {
        static QSGMaterialType type;
        return &type;
    }
    [[nodiscard]] QSGMaterialShader* createShader(QSGRendererInterface::RenderMode mode) const override;
    [[nodiscard]] int compare(const QSGMaterial* other) const override
    {
        return this == other ? 0 : (std::less<const QSGMaterial*>()(this, other) ? -1 : 1);
    }

    // Puts the newest picture in the planes when its node is drawn, where the GPU's resources are at hand.
    void prepare(QRhi& rhi, QRhiResourceUpdateBatch& updates)
    {
        if (frame == m_shown)
            return;
        m_shown = frame;
        if (!take(rhi, updates) && frame->onGpu()) {
            if (const auto gpu = videoGpu(); gpu && !gpu->failed.exchange(true))
                qCWarning(kodosiVideo) << "Qt Quick cannot show the pictures of this GPU, so video decodes in software";
        }
        // Planes that could not take a picture may have let their textures go.
        if (!m_planes || m_planes->luma() == nullptr) {
            auto blank = std::make_unique<MemoryPlanes>();
            if (blank->black(rhi, updates)) {
                m_planes = std::move(blank);
                m_planesOnGpu = false;
            }
        }
        // Qt Quick samples these next, so they must not point at a texture deleted above.
        luma.hold(m_planes ? m_planes->luma() : nullptr);
        chroma.hold(m_planes ? m_planes->chroma() : nullptr);
    }

    std::shared_ptr<const VideoFrame> frame;
    PlaneTexture luma;
    PlaneTexture chroma;

private:
    // Planes of the other kind replace the ones on screen only once they hold the picture, so the last
    // picture stays when they cannot.
    bool take(QRhi& rhi, QRhiResourceUpdateBatch& updates)
    {
        const bool onGpu = frame->onGpu();
        if (m_planes && m_planesOnGpu == onGpu)
            return m_planes->take(rhi, updates, frame);
        auto planes = onGpu ? gpuPlanes(rhi) : std::make_unique<MemoryPlanes>();
        if (!planes || !planes->take(rhi, updates, frame))
            return false;
        m_planes = std::move(planes);
        m_planesOnGpu = onGpu;
        return true;
    }

    std::shared_ptr<const VideoFrame> m_shown;
    std::unique_ptr<VideoPlanes> m_planes;
    bool m_planesOnGpu = false;
};

class VideoShader final : public QSGMaterialShader {
public:
    VideoShader()
    {
        setShaderFileName(VertexStage, QStringLiteral(":/kodosi/shaders/video.vert.qsb"));
        setShaderFileName(FragmentStage, QStringLiteral(":/kodosi/shaders/video.frag.qsb"));
    }

    bool updateUniformData(RenderState& state, QSGMaterial*, QSGMaterial*) override
    {
        auto* uniforms = state.uniformData();
        bool changed = false;
        if (state.isMatrixDirty()) {
            const auto matrix = state.combinedMatrix();
            std::memcpy(uniforms->data(), matrix.constData(), matrixBytes);
            changed = true;
        }
        if (state.isOpacityDirty()) {
            const float opacity = state.opacity();
            std::memcpy(uniforms->data() + matrixBytes, &opacity, sizeof(opacity));
            changed = true;
        }
        return changed;
    }

    void updateSampledImage(
        RenderState& state, int binding, QSGTexture** texture, QSGMaterial* newMaterial, QSGMaterial*) override
    {
        auto* material = static_cast<VideoMaterial*>(newMaterial);
        if (binding == lumaBinding)
            material->prepare(*state.rhi(), *state.resourceUpdateBatch());
        *texture = binding == lumaBinding ? &material->luma : &material->chroma;
    }
};

QSGMaterialShader* VideoMaterial::createShader(QSGRendererInterface::RenderMode) const
{
    return new VideoShader;
}
}

VideoNode::VideoNode()
{
    auto* geometry = new QSGGeometry(QSGGeometry::defaultAttributes_TexturedPoint2D(), 4);
    geometry->setDrawingMode(QSGGeometry::DrawTriangleStrip);
    setGeometry(geometry);
    setFlag(OwnsGeometry);
    setMaterial(new VideoMaterial);
    setFlag(OwnsMaterial);
}

void VideoNode::show(std::shared_ptr<const VideoFrame> frame, const QRectF& rect)
{
    auto* video = static_cast<VideoMaterial*>(material());
    if (video->frame != frame) {
        video->frame = std::move(frame);
        markDirty(DirtyMaterial);
    }
    if (rect != m_rect) {
        m_rect = rect;
        QSGGeometry::updateTexturedRectGeometry(geometry(), rect, QRectF(0, 0, 1, 1));
        markDirty(DirtyGeometry);
    }
}
}
