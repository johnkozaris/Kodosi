#include "canvas/CanvasImages.hpp"
#include "presentation/CanvasModel.hpp"

#include <QBuffer>
#include <QImageReader>

namespace kodosi {
CanvasImages::CanvasImages(CanvasModel& canvas)
    : QQuickImageProvider(QQuickImageProvider::Image)
    , m_canvas(canvas)
{
}

QImage CanvasImages::requestImage(const QString& id, QSize* size, const QSize& requestedSize)
{
    const auto parts = id.split(QLatin1Char('/'));
    QImage image;
    if (parts.size() == 3 && parts[0] == QStringLiteral("browser")) {
        image = m_canvas.browserImage(parts[1]);
    } else if (parts.size() == 2 && parts[0] == QStringLiteral("item")) {
        if (const auto content = m_canvas.published(parts[1].toInt())) {
            QBuffer buffer;
            buffer.setData(*content);
            QImageReader reader(&buffer);
            reader.setAutoTransform(true);
            const auto natural = reader.size();
            if (requestedSize.isValid() && natural.isValid()
                && (reader.format() == "svg" || natural.width() > requestedSize.width()
                    || natural.height() > requestedSize.height()))
                reader.setScaledSize(natural.scaled(requestedSize, Qt::KeepAspectRatio));
            image = reader.read();
        }
    }
    if (size)
        *size = image.size();
    return image;
}
}
