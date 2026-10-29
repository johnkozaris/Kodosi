#pragma once

#include <QQuickImageProvider>

namespace kodosi {
class CanvasModel;

// The pictures of the canvas: image://canvas/item/<published number> for a picture item and
// image://canvas/browser/<terminal>/<sequence> for the shared browser.
class CanvasImages final : public QQuickImageProvider {
public:
    explicit CanvasImages(CanvasModel& canvas);

    QImage requestImage(const QString& id, QSize* size, const QSize& requestedSize) override;

private:
    CanvasModel& m_canvas;
};
}
