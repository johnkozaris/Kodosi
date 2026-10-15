#pragma once

#include <QAbstractNativeEventFilter>
#include <QColor>
#include <QObject>

class QWindow;

namespace kodosi {

/// The native frame of a window whose title zone Kodosi draws. Create it
/// before the window's saved geometry is restored. It keeps the minimize,
/// maximize, snap and system menu behaviour that Qt drops with its own
/// buttons, lets the client area fill the visible window, and keeps Qt's
/// client geometry equal to the native one.
class WindowFrame final : public QObject, public QAbstractNativeEventFilter {
    Q_OBJECT

public:
    explicit WindowFrame(QWindow& window);
    ~WindowFrame() override;

    void setColor(const QColor& color);
    bool nativeEventFilter(const QByteArray& eventType, void* message, qintptr* result) override;

private:
    QWindow& m_window;
    void* m_handle = nullptr;
    QColor m_color = Qt::black;

    void alignClient();
    [[nodiscard]] bool fitClient(void* parameters) const;
    void paint() const;
};

}
