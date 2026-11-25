#pragma once

#include <QQuickImageProvider>

namespace kodosi {
class CanvasModel;

// Serves published canvas images via image://canvas/item/<id>.
class CanvasImages final : public QQuickImageProvider {
public:
    explicit CanvasImages(CanvasModel& canvas);

    QImage requestImage(const QString& id, QSize* size, const QSize& requestedSize) override;

private:
    CanvasModel& m_canvas;
};
}
