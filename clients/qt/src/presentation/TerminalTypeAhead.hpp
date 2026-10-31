#pragma once

#include <QEvent>
#include <QObject>
#include <QPointer>
#include <QSet>
#include <QString>
#include <QTimer>
#include <QWindow>

#include <memory>
#include <vector>

namespace kodosi {

// Keeps the keys that a person types while a new terminal opens, so that they reach that
// terminal and not the one before it.
class TerminalTypeAhead final : public QObject {
public:
    explicit TerminalTypeAhead(QObject* parent = nullptr);

    bool holding() const { return m_holding; }
    void hold();
    void expect(const QString& sessionId);
    void deliver(const QString& sessionId);
    void drop();

protected:
    bool eventFilter(QObject* target, QEvent* event) override;

private:
    QTimer m_expiry;
    QPointer<QWindow> m_window;
    QString m_terminal;
    QSet<int> m_pressed;
    std::vector<std::unique_ptr<QEvent>> m_keys;
    bool m_holding = false;
};

}
