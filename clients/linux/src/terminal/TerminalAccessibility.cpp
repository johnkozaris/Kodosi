#include "terminal/TerminalAccessibility.hpp"
#include "accessibility/AccessibilityScope.hpp"
#include "terminal/TerminalView.hpp"

#include <QAccessible>
#include <QAccessibleObject>
#include <QCoreApplication>
#include <QQuickWindow>

#include <algorithm>
#include <limits>

namespace kodosi {
namespace {

class TerminalAccessible final : public QAccessibleObject, public QAccessibleTextInterface {
public:
    explicit TerminalAccessible(TerminalView* view)
        : QAccessibleObject(view)
    {
    }

    QAccessibleInterface* parent() const override
    {
        const auto* view = terminalView();
        if (view == nullptr) {
            return nullptr;
        }
        for (auto* parent = view->parentItem(); parent != nullptr;
             parent = parent->parentItem()) {
            if (auto* accessible = QAccessible::queryAccessibleInterface(parent);
                accessible != nullptr) {
                return accessible;
            }
        }
        return view->window() == nullptr
            ? nullptr
            : QAccessible::queryAccessibleInterface(view->window());
    }

    QAccessibleInterface* child(int) const override { return nullptr; }
    int childCount() const override { return 0; }
    int indexOfChild(const QAccessibleInterface*) const override { return -1; }

    QString text(const QAccessible::Text type) const override
    {
        const auto* view = terminalView();
        if (view == nullptr) {
            return {};
        }
        switch (type) {
        case QAccessible::Name:
            return QCoreApplication::translate("TerminalAccessible", "Terminal");
        case QAccessible::Value:
            return view->accessibleText();
        case QAccessible::Description:
            return QCoreApplication::translate("TerminalAccessible", "Live terminal session");
        default:
            return {};
        }
    }

    QAccessible::Role role() const override { return QAccessible::Terminal; }

    QAccessible::State state() const override
    {
        QAccessible::State result;
        if (const auto* view = terminalView()) {
            const auto visible = view->isVisible() && view->opacity() > 0.0
                && view->window() != nullptr && view->window()->isVisible();
            result.focusable = true;
            result.focused = view->hasActiveFocus();
            result.disabled = !view->isEnabled() || !view->terminalReady();
            result.readOnly = view->readOnly();
            result.editable = !view->readOnly();
            result.multiLine = true;
            result.selectableText = true;
            result.invisible = !visible;
            result.offscreen = !visible
                || !rect().intersects(view->window()->geometry());
            result.active = view->hasActiveFocus();
        } else {
            result.invalid = object() == nullptr;
            result.invisible = true;
            result.offscreen = true;
            result.disabled = true;
        }
        return result;
    }

    QRect rect() const override
    {
        const auto* view = terminalView();
        if (view == nullptr || view->window() == nullptr) {
            return {};
        }
        const auto sceneRect = view->mapRectToScene(view->boundingRect());
        return QRect(
            view->window()->mapToGlobal(sceneRect.topLeft().toPoint()),
            sceneRect.size().toSize());
    }

    void* interface_cast(const QAccessible::InterfaceType type) override
    {
        return type == QAccessible::TextInterface
            ? static_cast<QAccessibleTextInterface*>(this)
            : QAccessibleObject::interface_cast(type);
    }

    void selection(int selectionIndex, int* startOffset, int* endOffset) const override
    {
        const auto* view = terminalView();
        const auto selection = view == nullptr ? QPair {-1, -1}
                                               : view->accessibleSelection();
        if (selectionIndex == 0 && selection.first >= 0) {
            *startOffset = selection.first;
            *endOffset = selection.second;
        } else {
            *startOffset = -1;
            *endOffset = -1;
        }
    }

    int selectionCount() const override
    {
        const auto* view = terminalView();
        return view != nullptr && view->accessibleSelection().first >= 0 ? 1 : 0;
    }

    void addSelection(int, int) override {}
    void removeSelection(int) override {}
    void setSelection(int, int, int) override {}
    int cursorPosition() const override
    {
        const auto* view = terminalView();
        return view == nullptr ? 0 : view->accessibleCursorPosition();
    }
    void setCursorPosition(int) override {}

    QString text(int startOffset, int endOffset) const override
    {
        const auto* view = terminalView();
        const auto value = view == nullptr ? QString {} : view->accessibleText();
        const auto size = static_cast<int>(std::min<qsizetype>(
            value.size(),
            std::numeric_limits<int>::max()));
        const auto start = std::clamp(startOffset, 0, size);
        const auto end = std::clamp(endOffset, start, size);
        return value.sliced(start, end - start);
    }

    int characterCount() const override
    {
        const auto* view = terminalView();
        return view == nullptr ? 0 : view->accessibleText().size();
    }
    QRect characterRect(int offset) const override
    {
        const auto* view = terminalView();
        return view == nullptr ? QRect {} : view->accessibleCharacterRect(offset);
    }
    int offsetAtPoint(const QPoint& point) const override
    {
        const auto* view = terminalView();
        return view == nullptr ? -1 : view->accessibleOffsetAt(point);
    }
    void scrollToSubstring(int, int) override {}
    QString attributes(int offset, int* startOffset, int* endOffset) const override
    {
        *startOffset = std::clamp(offset, 0, characterCount());
        *endOffset = *startOffset;
        return {};
    }

private:
    TerminalView* terminalView() const
    {
        auto* view = qobject_cast<TerminalView*>(object());
        return view != nullptr && !AccessibilityScope::isSuppressed(view) ? view : nullptr;
    }
};

QAccessibleInterface* terminalFactory(const QString&, QObject* object)
{
    auto* view = qobject_cast<TerminalView*>(object);
    return view == nullptr ? nullptr : new TerminalAccessible(view);
}

}

void installTerminalAccessibility()
{
    static const auto installed = [] {
        QAccessible::installFactory(terminalFactory);
        return true;
    }();
    Q_UNUSED(installed);
}

}
