#pragma once

#include <QObject>
#include <QtWebEngineQuick/QQuickWebEngineProfile>

namespace kodosi {
class CanvasModel;

// The closed frame of canvas pages: a profile that keeps nothing, serves only the pages that
// the canvas published and refuses every request to another place. Chromium stops the whole
// app when it cannot sandbox a page, so where the system refuses user namespaces or the app
// runs as root, pages do not show here.
class CanvasWeb final : public QObject {
    Q_OBJECT
    Q_PROPERTY(bool available READ available CONSTANT)
    Q_PROPERTY(QQuickWebEngineProfile* profile READ profile CONSTANT)

public:
    explicit CanvasWeb(CanvasModel& canvas, QObject* parent = nullptr);

    static void registerScheme();
    [[nodiscard]] bool available() const { return m_available; }
    [[nodiscard]] QQuickWebEngineProfile* profile();

private:
    CanvasModel& m_canvas;
    QQuickWebEngineProfile* m_profile = nullptr;
    bool m_available = false;
};
}
