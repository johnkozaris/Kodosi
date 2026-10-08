#include "accessibility/AccessibilityScope.hpp"

#include <QAccessible>
#include <QQuickWindow>
#include <QtQml/qqml.h>
#include <QtQuick/private/qaccessiblequickitem_p.h>
#include <QtQuick/private/qquickaccessibleattached_p.h>

namespace kodosi {
namespace {

class ScopeAccessible final : public QAccessibleQuickItem {
public:
    explicit ScopeAccessible(AccessibilityScope* scope)
        : QAccessibleQuickItem(scope)
    {
    }

    int childCount() const override
    {
        return blocked() ? 0 : QAccessibleQuickItem::childCount();
    }

    QAccessibleInterface* child(int index) const override
    {
        return blocked() ? nullptr : QAccessibleQuickItem::child(index);
    }

    int indexOfChild(const QAccessibleInterface* child) const override
    {
        return blocked() ? -1 : QAccessibleQuickItem::indexOfChild(child);
    }

    QAccessibleInterface* childAt(int x, int y) const override
    {
        return blocked() ? nullptr : QAccessibleQuickItem::childAt(x, y);
    }

    QAccessibleInterface* focusChild() const override
    {
        return blocked() ? nullptr : QAccessibleQuickItem::focusChild();
    }

    QAccessible::State state() const override
    {
        auto result = QAccessibleQuickItem::state();
        if (blocked()) {
            result.invisible = true;
            result.offscreen = true;
            result.disabled = true;
            result.focusable = false;
            result.focused = false;
        }
        return result;
    }

private:
    bool blocked() const { return AccessibilityScope::isSuppressed(item()); }
};

QAccessibleInterface* scopeFactory(const QString&, QObject* object)
{
    auto* scope = qobject_cast<AccessibilityScope*>(object);
    return scope == nullptr ? nullptr : new ScopeAccessible(scope);
}

}

AccessibilityScope::AccessibilityScope(QQuickItem* parent)
    : QQuickItem(parent)
{
    static const auto installed = [] {
        QAccessible::installFactory(scopeFactory);
        return true;
    }();
    Q_UNUSED(installed);


    auto* accessible = qobject_cast<QQuickAccessibleAttached*>(
        qmlAttachedPropertiesObject<QQuickAccessibleAttached>(this, true));
    accessible->setRole(QAccessible::Grouping);
    connect(this, &QQuickItem::visibleChanged, this, &AccessibilityScope::publishAccessibilityChange);
    connect(this, &QQuickItem::enabledChanged, this, &AccessibilityScope::publishAccessibilityChange);
    connect(this, &QQuickItem::opacityChanged, this, &AccessibilityScope::publishAccessibilityChange);
    connect(this, &QQuickItem::childrenChanged, this, &AccessibilityScope::publishAccessibilityChange);
    connect(this, &QQuickItem::parentChanged, this, &AccessibilityScope::publishAccessibilityChange);
}

bool AccessibilityScope::suppressed() const noexcept
{
    return m_suppressed;
}

void AccessibilityScope::setSuppressed(bool suppressed)
{
    if (m_suppressed == suppressed)
        return;
    m_suppressed = suppressed;
    emit suppressedChanged();
    publishAccessibilityChange();
}

bool AccessibilityScope::blocksAccessibility() const noexcept
{
    return m_suppressed || !isVisible() || !isEnabled() || opacity() <= 0.0;
}

bool AccessibilityScope::isSuppressed(const QQuickItem* item) noexcept
{
    for (auto* ancestor = item; ancestor != nullptr; ancestor = ancestor->parentItem()) {
        if (const auto* scope = qobject_cast<const AccessibilityScope*>(ancestor);
            scope != nullptr && scope->blocksAccessibility()) {
            return true;
        }
    }
    return false;
}

void AccessibilityScope::publishAccessibilityChange()
{
    if (!QAccessible::isActive())
        return;
    QAccessible::State changed;
    changed.invisible = true;
    changed.offscreen = true;
    changed.disabled = true;
    QAccessibleStateChangeEvent state(this, changed);
    QAccessible::updateAccessibility(&state);
    QAccessibleEvent children(this, QAccessible::ObjectReorder);
    QAccessible::updateAccessibility(&children);
}

}
