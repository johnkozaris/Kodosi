#include "terminal/TerminalView.hpp"
#include "logging/PerformanceSpan.hpp"

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
        forgetRaster();
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
    TerminalRasterizer::Options options;
    options.palette = m_palette;
    options.overlay.preedit = m_preedit;
    options.overlay.column = m_frame->cursor.column;
    options.overlay.row = m_frame->cursor.row;
    options.lineHeight = m_lineHeight;
    options.cursorStyle = m_cursorStyle;
    options.cursorPhaseVisible = m_cursorPhaseVisible;
    const auto sameRaster = m_renderedFrame && !m_renderedImage.isNull()
        && qFuzzyCompare(m_renderedDevicePixelRatio, devicePixelRatio)
        && m_renderedFont == m_font;
    if (nodeNeedsTexture || !sameRaster || m_renderedFrame != m_frame
        || m_renderedOptions != options
        || !qFuzzyCompare(m_renderedViewportScale, scale)) {
        QElapsedTimer rasterTimer;
        rasterTimer.start();
        const auto painted = sameRaster
            ? TerminalRasterizer::repaintChangedRows(
                  m_renderedImage,
                  *m_renderedFrame,
                  m_renderedOptions,
                  *m_frame,
                  m_font,
                  options)
            : std::nullopt;
        if (!painted) {
            m_renderedImage = TerminalRasterizer::render(
                *m_frame,
                m_font,
                devicePixelRatio,
                options);
        }
        auto image = m_renderedImage;
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
            forgetRaster();
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
            forgetRaster();
            delete node;
            return nullptr;
        }
        queueRenderPerformance(rasterTimer.elapsed(), QStringLiteral("ok"));
        node->setTexture(texture);
        m_renderedFrame = m_frame;
        m_renderedOptions = options;
        m_renderedDevicePixelRatio = devicePixelRatio;
        m_renderedViewportScale = scale;
        m_renderedFont = m_font;
    }
    const auto logicalSize = gridSize() * scale;
    node->setRect(QRectF(viewportOffset(), logicalSize));
    node->setFiltering(scale == 1.0 ? QSGTexture::Nearest : QSGTexture::Linear);
    return node;
}

void TerminalView::forgetRaster()
{
    m_renderedFrame.reset();
    m_renderedImage = {};
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
