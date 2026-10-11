#pragma once

#include <QElapsedTimer>
#include <QLoggingCategory>
#include <QString>
#include <QStringView>

Q_DECLARE_LOGGING_CATEGORY(kodosiPerformance)

namespace kodosi {

class ScopedPerformanceSpan final {
public:
    ScopedPerformanceSpan(QStringView operation, qint64 thresholdMilliseconds);
    ~ScopedPerformanceSpan();

    ScopedPerformanceSpan(const ScopedPerformanceSpan&) = delete;
    ScopedPerformanceSpan& operator=(const ScopedPerformanceSpan&) = delete;

    void setOutcome(QStringView outcome);

private:
    QString m_operation;
    QString m_outcome = QStringLiteral("ok");
    qint64 m_thresholdMilliseconds = 0;
    QElapsedTimer m_timer;
};

}
