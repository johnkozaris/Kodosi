#include "logging/PerformanceSpan.hpp"

#include <algorithm>

Q_LOGGING_CATEGORY(kodosiPerformance, "kodosi.performance")

namespace kodosi {
namespace {

QString performanceCategoryName(const PerformanceCategory category)
{
    switch (category) {
    case PerformanceCategory::Terminal:
        return QStringLiteral("terminal");
    }
    return QStringLiteral("unknown");
}

}

ScopedPerformanceSpan::ScopedPerformanceSpan(
    const PerformanceCategory category,
    QStringView operation,
    const qint64 thresholdMilliseconds)
    : m_category(category)
    , m_operation(operation.left(64).toString())
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
        << QStringLiteral("category=%1 operation=%2 duration_ms=%3 outcome=%4")
               .arg(
                   performanceCategoryName(m_category),
                   m_operation,
                   QString::number(elapsed),
                   m_outcome);
}

void ScopedPerformanceSpan::setOutcome(QStringView outcome)
{
    m_outcome = outcome.left(32).toString();
}

}
