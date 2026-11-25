#pragma once

#include <QObject>
#include <QtWebEngineQuick/QQuickWebEngineProfile>

namespace kodosi {
class CanvasModel;

// Manages isolated off-the-record WebEngine profiles that serve only published canvas pages and
// bundled document views while blocking external requests. Each canvas page gets its own profile
// so storage is isolated per view and discarded when the view closes. Chromium aborts the process
// if it cannot initialize its sandbox, so WebEngine views are disabled when user namespaces are
// unavailable or the app runs as root.
class CanvasWeb final : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool available READ available CONSTANT FINAL)
    Q_PROPERTY(QQuickWebEngineProfile* documentProfile READ documentProfile CONSTANT FINAL)

public:
    explicit CanvasWeb(CanvasModel& canvas, QObject* parent = nullptr);

    static void registerScheme();
    [[nodiscard]] bool available() const { return m_available; }
    [[nodiscard]] QQuickWebEngineProfile* documentProfile();
    Q_INVOKABLE QQuickWebEngineProfile* pageProfile(QObject* view);

private:
    CanvasModel& m_canvas;
    QQuickWebEngineProfile* m_documentProfile = nullptr;
    bool m_available = false;
};
}
