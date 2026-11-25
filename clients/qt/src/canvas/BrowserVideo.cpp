#include "canvas/BrowserVideo.hpp"
#include "presentation/CanvasModel.hpp"

#include <QClipboard>
#include <QGuiApplication>
#include <QSet>

namespace kodosi {
BrowserVideo::BrowserVideo(QQuickItem* parent)
    : StreamVideo(QStringLiteral("browser"), parent)
{
}

void BrowserVideo::take(const QJsonObject& message)
{
    m_window = message.toVariantMap();
    emit windowChanged();
    keepIcons(message.value(QStringLiteral("icons")).toObject(), message.value(QStringLiteral("tabs")).toArray());
    const auto copied = message.value(QStringLiteral("copied")).toString();
    if (!copied.isEmpty())
        QGuiApplication::clipboard()->setText(copied);
}

void BrowserVideo::forget()
{
    if (!m_window.isEmpty()) {
        m_window.clear();
        emit windowChanged();
    }
    if (!m_icons.isEmpty()) {
        m_icons.clear();
        emit iconsChanged();
    }
}

void BrowserVideo::keepIcons(const QJsonObject& changed, const QJsonArray& tabs)
{
    auto kept = m_icons;
    for (auto icon = changed.begin(); icon != changed.end(); ++icon) {
        const auto png = icon.value().toString();
        if (png.isEmpty())
            kept.remove(icon.key());
        else
            kept.insert(icon.key(), QStringLiteral("data:image/png;base64,") + png);
    }
    QSet<QString> open;
    for (const auto& tab : tabs)
        open.insert(tab.toObject().value(QStringLiteral("id")).toString());
    for (auto icon = kept.begin(); icon != kept.end();)
        icon = open.contains(icon.key()) ? std::next(icon) : kept.erase(icon);
    if (kept != m_icons) {
        m_icons = kept;
        emit iconsChanged();
    }
}

QSizeF BrowserVideo::browserSize() const
{
    return QSizeF(m_window.value(QStringLiteral("width")).toReal(), m_window.value(QStringLiteral("height")).toReal());
}

std::optional<QPointF> BrowserVideo::streamPoint(QPointF point, bool clamped) const
{
    return CanvasModel::browserPoint(point, size(), browserSize(), clamped);
}

QVariant BrowserVideo::viewPoint(qreal x, qreal y) const
{
    const auto at = CanvasModel::pointInView(QPointF(x, y), size(), browserSize());
    return at ? QVariant(*at) : QVariant {};
}
}
