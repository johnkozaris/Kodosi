#pragma once

#include <QObject>
#include <QtQml/qqmlregistration.h>

#include <chrono>
#include <cstdint>
#include <functional>

namespace kodosi {

// Shared tick clock for looping indicators so they repaint together on a single frame with no
// intermediate draws. Software-rendered windows tick less often to reduce full-frame CPU cost.
class SignClock final {
public:
    static constexpr std::chrono::milliseconds step { 75 };
    static constexpr std::chrono::milliseconds processorStep { 250 };

    static void join(const QObject* sign, std::function<void(std::int64_t now)> onStep);
    static void leave(const QObject* sign);
    [[nodiscard]] static bool running();
    [[nodiscard]] static std::int64_t now();
};

// Normalized [0, 1) loop progress driven by SignClock since running started.
class SignPhase : public QObject {
    Q_OBJECT
    QML_ELEMENT
    Q_PROPERTY(bool running READ running WRITE setRunning NOTIFY runningChanged FINAL)
    Q_PROPERTY(int period READ period WRITE setPeriod NOTIFY periodChanged FINAL)
    Q_PROPERTY(double value READ value NOTIFY valueChanged FINAL)

public:
    explicit SignPhase(QObject* parent = nullptr);
    ~SignPhase() override;

    [[nodiscard]] bool running() const { return m_running; }
    void setRunning(bool running);
    [[nodiscard]] int period() const { return m_period; }
    void setPeriod(int period);
    [[nodiscard]] double value() const { return m_value; }

signals:
    void runningChanged();
    void periodChanged();
    void valueChanged();

private:
    void follow(std::int64_t now);

    bool m_running = false;
    int m_period = 1000;
    std::int64_t m_start = 0;
    double m_value = 0;
};

}
