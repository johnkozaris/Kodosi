#pragma once

#include <QDebug>
#include <QString>

#include <utility>

namespace kodosi {

class RuntimeEventFaults final {
public:
    void unreadable(const QString& type, const QString& reason)
    {
        const auto fault = type.isEmpty() ? QStringLiteral("the app could not read an event: %1").arg(reason)
                                          : QStringLiteral("the app could not read the event %1: %2").arg(type, reason);
        if (std::exchange(m_last, fault) != fault)
            qWarning().noquote() << fault;
    }

private:
    QString m_last;
};

}
