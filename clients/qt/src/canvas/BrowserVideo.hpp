#pragma once

#include "canvas/StreamVideo.hpp"

#include <QJsonArray>
#include <QVariantMap>

namespace kodosi {
// The shared browser of a terminal's canvas: its video, and its tabs, pointers, menus and dialogs.
class BrowserVideo : public StreamVideo {
    Q_OBJECT
    Q_PROPERTY(QVariantMap window READ window NOTIFY windowChanged FINAL)
    Q_PROPERTY(QVariantMap icons READ icons NOTIFY iconsChanged FINAL)

public:
    explicit BrowserVideo(QQuickItem* parent = nullptr);

    [[nodiscard]] QVariantMap window() const { return m_window; }
    [[nodiscard]] QVariantMap icons() const { return m_icons; }

    Q_INVOKABLE [[nodiscard]] QVariant viewPoint(qreal x, qreal y) const;

signals:
    void windowChanged();
    void iconsChanged();

protected:
    void take(const QJsonObject& message) override;
    [[nodiscard]] std::optional<QPointF> streamPoint(QPointF point, bool clamped) const override;
    void forget() override;

private:
    void keepIcons(const QJsonObject& changed, const QJsonArray& tabs);
    [[nodiscard]] QSizeF browserSize() const;

    QVariantMap m_window;
    QVariantMap m_icons;
};
}
