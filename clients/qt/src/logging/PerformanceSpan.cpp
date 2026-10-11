#include "logging/PerformanceSpan.hpp"

#include <algorithm>

Q_LOGGING_CATEGORY(kodosiPerformance, "kodosi.performance")

namespace kodosi {

ScopedPerformanceSpan::ScopedPerformanceSpan(QStringView operation, const qint64 thresholdMilliseconds)
    : m_operation(operation.left(64).toString())
    , m_thresholdMilliseconds(std::max<qint64>(0, thresholdMilliseconds))
{
    m_timer.start();
}

ScopedPerformanceSpan::~ScopedPerformanceSpan()
{
    const auto elapsed = m_timer.elapsed();
    if (elapsed < m_thresholdMilliseconds) {
        return;
    }
    qCInfo(kodosiPerformance).noquote()
        << QStringLiteral("category=terminal operation=%1 duration_ms=%2 outcome=%3")
               .arg(m_operation, QString::number(elapsed), m_outcome);
}

void ScopedPerformanceSpan::setOutcome(QStringView outcome)
{
    m_outcome = outcome.left(32).toString();
}

}
