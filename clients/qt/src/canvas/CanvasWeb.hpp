#pragma once

#include <QObject>
#include <QtWebEngineQuick/QQuickWebEngineProfile>

namespace kodosi {
class CanvasModel;

// The closed frames of the canvas: profiles that keep nothing, serve only the pages that the
// canvas published and the views of the app, and refuse every request to another place. Each page
// has a profile of its own, so no page reads what the page of another terminal stored, and the
// storage of a page ends with its view. Chromium stops the whole app when it cannot sandbox a
// page, so where the system refuses user namespaces or the app runs as root, pages do not show
// here.
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
