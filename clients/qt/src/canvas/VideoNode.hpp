#pragma once

#include <QRectF>
#include <QSGGeometryNode>

#include <memory>

class QRhi;
class QRhiResourceUpdateBatch;
class QRhiTexture;

namespace kodosi {
class VideoFrame;

// A picture's planes where the GPU samples them: luma as R8, and chroma as RG8 with Cb in red and Cr in
// green.
class VideoPlanes {
public:
    virtual ~VideoPlanes() = default;
    // Puts `frame` in the planes; false when this path cannot show it.
    virtual bool take(QRhi& rhi, QRhiResourceUpdateBatch& updates, std::shared_ptr<const VideoFrame> frame) = 0;
    [[nodiscard]] virtual QRhiTexture* luma() const = 0;
    [[nodiscard]] virtual QRhiTexture* chroma() const = 0;
};

// The planes of the pictures that this platform's GPU decodes, or nothing when Qt Quick draws with an
// API that cannot sample them.
std::unique_ptr<VideoPlanes> gpuPlanes(QRhi& rhi);

// Shows decoded pictures through a shader that turns YUV into RGB, so the processor never touches a
// picture that the GPU decoded.
class VideoNode final : public QSGGeometryNode {
public:
    VideoNode();
    // `rest` dims and greys the picture, from 0 for none to 1 for a picture that is not live.
    void show(std::shared_ptr<const VideoFrame> frame, const QRectF& rect, float rest);

private:
    QRectF m_rect;
};
}
