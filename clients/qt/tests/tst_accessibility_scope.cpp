#include "accessibility/AccessibilityScope.hpp"
#include "terminal/GhosttyC.hpp"
#include "terminal/TerminalAccessibility.hpp"
#include "terminal/TerminalSessionRegistry.hpp"
#include "terminal/TerminalView.hpp"

#include <QAccessible>
#include <QQmlComponent>
#include <QQmlEngine>
#include <QQuickWindow>
#include <QSet>
#include <QtTest/QTest>

#include <memory>

namespace {

QSet<QString> accessibleNames(QAccessibleInterface* root)
{
    QSet<QString> names;
    if (root == nullptr)
        return names;
    names.insert(root->text(QAccessible::Name));
    for (int index = 0; index < root->childCount(); ++index)
        names.unite(accessibleNames(root->child(index)));
    return names;
}

class TerminalDispatcher final : public kodosi::TerminalCommandDispatcher {
public:
    bool isRunning() const noexcept override { return true; }
    Result send(QByteArrayView) override { return {}; }
    Result connectTerminal(const kodosi::TerminalSubscription&) override { return {}; }
    Result refreshTerminal(const kodosi::TerminalSubscription&) override { return {}; }
    Result disconnectTerminal(const kodosi::TerminalSubscription&) override { return {}; }
    Result sendTerminalInput(const kodosi::TerminalSubscription&, const QString&, QByteArrayView) override
    { return {}; }
};

QByteArray checkpoint()
{
    GhosttyTerminal terminal = nullptr;
    if (ghostty_terminal_new(nullptr, &terminal, 40, 6) != GHOSTTY_SUCCESS)
        return {};
    const auto bytes = QByteArrayLiteral("private terminal contents");
    ghostty_terminal_vt_write(terminal, reinterpret_cast<const std::uint8_t*>(bytes.constData()),
        static_cast<std::size_t>(bytes.size()));
    const GhosttyCheckpointEncodeOptions options = GHOSTTY_CHECKPOINT_ENCODE_OPTIONS_INIT;
    GhosttyBuffer buffer {};
    GhosttyCheckpointInfo info = GHOSTTY_INIT_SIZED(GhosttyCheckpointInfo);
    if (ghostty_checkpoint_encode_buf(terminal, &options, &buffer, &info) != GHOSTTY_OUT_OF_SPACE) {
        ghostty_terminal_free(terminal);
        return {};
    }
    QByteArray bytesOut(static_cast<qsizetype>(buffer.len), Qt::Uninitialized);
    buffer.ptr = reinterpret_cast<std::uint8_t*>(bytesOut.data());
    buffer.cap = static_cast<std::size_t>(bytesOut.size());
    buffer.len = 0;
    const auto result = ghostty_checkpoint_encode_buf(terminal, &options, &buffer, &info);
    ghostty_terminal_free(terminal);
    return result == GHOSTTY_SUCCESS ? bytesOut : QByteArray {};
}

}

class AccessibilityScopeTest final : public QObject {
    Q_OBJECT
private slots:
    void scopesFilterNativeTreeWithoutOverwritingChildBindings();
    void cachedTerminalInterfaceCannotReadSuppressedSubtree();
};

void AccessibilityScopeTest::scopesFilterNativeTreeWithoutOverwritingChildBindings()
{
    qmlRegisterType<kodosi::AccessibilityScope>("Kodosi.Accessibility.Test", 1, 0, "AccessibilityScope");
    QQmlEngine engine;
    QQmlComponent component(&engine);
    component.setData(R"(
        import QtQuick
        import QtQuick.Controls
        import Kodosi.Accessibility.Test
        ApplicationWindow {
            id: window
            width: 640; height: 480; visible: true
            readonly property bool overlayOpen: overlay.opened
            property alias overlay: overlay
            property bool sidebarOpen: true
            property bool hiddenIgnored: true
            property int dynamicCount: 0
            property alias scope: shell
            property alias ignoredChild: hidden
            AccessibilityScope {
                id: shell
                anchors.fill: parent
                suppressed: window.overlayOpen
                Accessible.name: "Shell"
                Button { text: "Background"; Accessible.name: "Background" }
                Button {
                    id: hidden
                    y: 50
                    text: "Individually ignored"
                    Accessible.name: "Individually ignored"
                    Accessible.ignored: window.hiddenIgnored
                }
                AccessibilityScope {
                    x: 200; width: 200; height: 400
                    visible: window.sidebarOpen
                    Accessible.name: "Sidebar"
                    Button { text: "Sidebar action"; Accessible.name: "Sidebar action" }
                    Repeater {
                        model: window.dynamicCount
                        Button {
                            required property int index
                            y: 50 + index * 40
                            text: "Dynamic " + index
                            Accessible.name: text
                        }
                    }
                }
            }
            Popup {
                id: overlay
                parent: Overlay.overlay
                x: 420; y: 20; width: 180; height: 80
                modal: true
                Button {
                    anchors.fill: parent
                    text: "Overlay action"
                    Accessible.name: "Overlay action"
                }
            }
        }
    )", {});
    QVERIFY2(component.isReady(), qPrintable(component.errorString()));
    std::unique_ptr<QObject> root(component.create());
    QVERIFY2(root != nullptr, qPrintable(component.errorString()));
    auto* window = qobject_cast<QQuickWindow*>(root.get());
    QVERIFY(window);
    auto* scope = root->property("scope").value<kodosi::AccessibilityScope*>();
    QVERIFY(scope);
    auto* tree = QAccessible::queryAccessibleInterface(window);
    auto* boundary = QAccessible::queryAccessibleInterface(scope);
    QVERIFY(tree);
    QVERIFY(boundary);
    QTRY_VERIFY(accessibleNames(tree).contains(QStringLiteral("Background")));
    QVERIFY(accessibleNames(tree).contains(QStringLiteral("Sidebar action")));
    QVERIFY(!accessibleNames(tree).contains(QStringLiteral("Individually ignored")));

    auto* overlay = root->property("overlay").value<QObject*>();
    QVERIFY(overlay);
    QVERIFY(QMetaObject::invokeMethod(overlay, "open"));
    QTRY_VERIFY(overlay->property("opened").toBool());
    QCOMPARE(boundary->childCount(), 0);
    QVERIFY(boundary->child(0) == nullptr);
    QVERIFY(boundary->childAt(20, 20) == nullptr);
    QVERIFY(boundary->state().invisible);
    QVERIFY(!accessibleNames(tree).contains(QStringLiteral("Background")));
    QVERIFY(!accessibleNames(tree).contains(QStringLiteral("Sidebar action")));
    QVERIFY(accessibleNames(tree).contains(QStringLiteral("Overlay action")));
    root->setProperty("dynamicCount", 2);
    root->setProperty("hiddenIgnored", false);
    QCoreApplication::processEvents();
    QVERIFY(!accessibleNames(tree).contains(QStringLiteral("Dynamic 0")));
    QVERIFY(!accessibleNames(tree).contains(QStringLiteral("Individually ignored")));

    QVERIFY(QMetaObject::invokeMethod(overlay, "close"));
    QTRY_VERIFY(!overlay->property("visible").toBool());
    QTRY_VERIFY(accessibleNames(tree).contains(QStringLiteral("Dynamic 0")));
    QVERIFY(accessibleNames(tree).contains(QStringLiteral("Dynamic 1")));
    QVERIFY(accessibleNames(tree).contains(QStringLiteral("Individually ignored")));
    root->setProperty("hiddenIgnored", true);
    QVERIFY(!accessibleNames(tree).contains(QStringLiteral("Individually ignored")));

    root->setProperty("sidebarOpen", false);
    QVERIFY(!accessibleNames(tree).contains(QStringLiteral("Sidebar action")));
    QVERIFY(!accessibleNames(tree).contains(QStringLiteral("Dynamic 0")));
    QVERIFY(accessibleNames(tree).contains(QStringLiteral("Background")));
    root->setProperty("dynamicCount", 3);
    QCoreApplication::processEvents();
    QVERIFY(!accessibleNames(tree).contains(QStringLiteral("Dynamic 2")));
    root->setProperty("sidebarOpen", true);
    QTRY_VERIFY(accessibleNames(tree).contains(QStringLiteral("Dynamic 2")));

    scope->setEnabled(false);
    QCOMPARE(boundary->childCount(), 0);
    scope->setEnabled(true);
    QVERIFY(boundary->childCount() > 0);
    QVERIFY(!accessibleNames(tree).contains(QStringLiteral("Individually ignored")));
}

void AccessibilityScopeTest::cachedTerminalInterfaceCannotReadSuppressedSubtree()
{
    kodosi::installTerminalAccessibility();
    kodosi::TerminalSessionRegistry registry;
    TerminalDispatcher dispatcher;
    QQuickWindow window;
    window.resize(700, 400);
    kodosi::AccessibilityScope scope(window.contentItem());
    scope.setWidth(700);
    scope.setHeight(400);
    kodosi::TerminalView terminal(&scope);
    terminal.setWidth(600);
    terminal.setHeight(300);
    const kodosi::TerminalSubscription subscription {
        QStringLiteral("session"), QStringLiteral("subscription"), 7,
    };
    QVERIFY(terminal.attach(registry, dispatcher, subscription, QStringLiteral("incarnation")));
    const auto bytes = checkpoint();
    QVERIFY(!bytes.isEmpty());
    QVERIFY(registry.installSemanticCheckpoint({subscription, 1, 6, 40, bytes}));
    registry.receiveConnectResult({subscription, 0});
    window.show();
    auto* accessible = QAccessible::queryAccessibleInterface(&terminal);
    auto* boundary = QAccessible::queryAccessibleInterface(&scope);
    QVERIFY(accessible);
    QVERIFY(accessible->textInterface());
    QTRY_VERIFY(accessible->text(QAccessible::Value).contains(QStringLiteral("private terminal contents")));
    QVERIFY(accessibleNames(boundary).contains(QStringLiteral("Terminal")));

    scope.setSuppressed(true);
    QCOMPARE(boundary->childCount(), 0);
    QVERIFY(accessible->state().invisible);
    QVERIFY(accessible->text(QAccessible::Value).isEmpty());
    QCOMPARE(accessible->textInterface()->characterCount(), 0);
    QVERIFY(accessible->textInterface()->text(0, 100).isEmpty());
    QVERIFY(accessible->rect().isEmpty());
    QVERIFY(accessible->parent() == nullptr);
    scope.setSuppressed(false);
    QTRY_VERIFY(accessible->text(QAccessible::Value).contains(QStringLiteral("private terminal contents")));
    scope.setVisible(false);
    QVERIFY(accessible->text(QAccessible::Value).isEmpty());
    scope.setVisible(true);
    QVERIFY(accessible->textInterface()->characterCount() > 0);
    terminal.detach();
}

QTEST_MAIN(AccessibilityScopeTest)
#include "tst_accessibility_scope.moc"
