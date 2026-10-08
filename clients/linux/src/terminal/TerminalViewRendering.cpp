#include "terminal/TerminalView.hpp"
#include "logging/ApplicationLogStore.hpp"

#include <QQuickWindow>
#include <QSGSimpleTextureNode>
#include <QSGTexture>

#include <algorithm>

namespace kodosi {

QSGNode* TerminalView::updatePaintNode(
    QSGNode* oldNode,
    UpdatePaintNodeData* updateData)
{
    Q_UNUSED(updateData);
    auto* node = static_cast<QSGSimpleTextureNode*>(oldNode);
    if (!m_frame || window() == nullptr || width() <= 0.0 || height() <= 0.0) {
        m_renderedFrame.reset();
        m_renderedDevicePixelRatio = 0.0;
        delete node;
        return nullptr;
    }
    const auto nodeNeedsTexture = node == nullptr || node->texture() == nullptr;
    if (node == nullptr) {
        node = new QSGSimpleTextureNode();
        node->setOwnsTexture(true);
    }

    const auto devicePixelRatio = window()->effectiveDevicePixelRatio();
    const auto scale = viewportScale();
    if (nodeNeedsTexture || m_renderedFrame != m_frame
        || !qFuzzyCompare(m_renderedDevicePixelRatio, devicePixelRatio)
        || !qFuzzyCompare(m_renderedViewportScale, scale)
        || m_renderedFontPixelSize != m_font.pixelSize()
        || m_renderedPreedit != m_preedit) {
        TerminalRasterizer::Options options;
        options.palette = m_palette;
        options.overlay.preedit = m_preedit;
        options.overlay.column = m_frame->cursor.column;
        options.overlay.row = m_frame->cursor.row;
        options.lineHeight = m_lineHeight;
        options.cursorStyle = m_cursorStyle;
        options.cursorPhaseVisible = m_cursorPhaseVisible;
        QElapsedTimer rasterTimer;
        rasterTimer.start();
        auto image = TerminalRasterizer::render(
            *m_frame,
            m_font,
            devicePixelRatio,
            options);
        if (!image.isNull() && scale < 1.0) {
            image = image.scaled(
                QSize(std::max(1, qRound(image.width() * scale)),
                    std::max(1, qRound(image.height() * scale))),
                Qt::IgnoreAspectRatio,
                Qt::SmoothTransformation);
        }
        if (image.isNull()) {
            queueRenderPerformance(
                rasterTimer.elapsed(),
                QStringLiteral("invalid"));
            delete node;
            return nullptr;
        }
        auto* texture = window()->createTextureFromImage(
            image,
            QQuickWindow::TextureHasAlphaChannel);
        if (texture == nullptr) {
            queueRenderPerformance(
                rasterTimer.elapsed(),
                QStringLiteral("texture-failed"));
            delete node;
            return nullptr;
        }
        queueRenderPerformance(rasterTimer.elapsed(), QStringLiteral("ok"));
        node->setTexture(texture);
        m_renderedFrame = m_frame;
        m_renderedDevicePixelRatio = devicePixelRatio;
        m_renderedViewportScale = scale;
        m_renderedFontPixelSize = m_font.pixelSize();
        m_renderedPreedit = m_preedit;
    }
    const auto logicalSize = gridSize() * scale;
    node->setRect(QRectF(viewportOffset(), logicalSize));
    node->setFiltering(scale == 1.0 ? QSGTexture::Nearest : QSGTexture::Linear);
    return node;
}

void TerminalView::queueRenderPerformance(
    const qint64 durationMilliseconds,
    QString outcome)
{
    if (durationMilliseconds < 16
        || m_renderPerformanceQueued.exchange(true, std::memory_order_acq_rel)) {
        return;
    }
    const auto generation =
        m_renderPerformanceGeneration.load(std::memory_order_acquire);
    const QPointer<TerminalView> self(this);
    QMetaObject::invokeMethod(
        this,
        [self,
         generation,
         durationMilliseconds = std::clamp<qint64>(
             durationMilliseconds,
             0,
             60'000),
         outcome = outcome.left(32)] {
            if (self == nullptr) {
                return;
            }
            if (self->m_renderPerformanceGeneration.load(
                       std::memory_order_acquire)
                    != generation) {
                self->m_renderPerformanceQueued.store(
                    false,
                    std::memory_order_release);
                return;
            }
            self->m_renderPerformanceQueued.store(
                false,
                std::memory_order_release);
            qCInfo(kodosiPerformance).noquote()
                << QStringLiteral(
                       "category=terminal operation=frame.raster "
                       "duration_ms=%1 outcome=%2")
                       .arg(
                           QString::number(durationMilliseconds),
                           outcome);
        },
        Qt::QueuedConnection);
}

}
