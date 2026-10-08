#pragma once

#include <QQuickItem>

namespace kodosi {



class AccessibilityScope : public QQuickItem {
    Q_OBJECT
    Q_PROPERTY(bool suppressed READ suppressed WRITE setSuppressed NOTIFY suppressedChanged)

public:
    explicit AccessibilityScope(QQuickItem* parent = nullptr);
    [[nodiscard]] bool suppressed() const noexcept;
    void setSuppressed(bool suppressed);
    [[nodiscard]] bool blocksAccessibility() const noexcept;
    [[nodiscard]] static bool isSuppressed(const QQuickItem* item) noexcept;

signals:
    void suppressedChanged();

private:
    bool m_suppressed = false;
    void publishAccessibilityChange();
};

}
