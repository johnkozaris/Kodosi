#include "presentation/SessionCatalogModel.hpp"
#include <QJsonArray>
#include <QUuid>

namespace test {
inline QString id(int value)
{
    return QStringLiteral("0198aaaa-0000-7000-8000-%1").arg(value, 12, 16, QChar(u'0'));
}
inline QJsonObject session(int value, bool remote = false, bool owner = true)
{
    return { { QStringLiteral("id"), id(value) }, { QStringLiteral("incarnationId"), id(value + 100) },
        { QStringLiteral("name"), QStringLiteral("Terminal") },
        { QStringLiteral("kind"), remote ? QStringLiteral("remote") : QStringLiteral("local") },
        { QStringLiteral("workingDir"), QStringLiteral("/repo") }, { QStringLiteral("isOwner"), owner },
        { QStringLiteral("status"), QStringLiteral("running") },
        { QStringLiteral("connectionState"), remote ? QStringLiteral("connected") : QStringLiteral("local") },
        { QStringLiteral("sharedWith"), QJsonArray {} } };
}
inline QJsonObject snapshot(QJsonArray entries)
{
    return { { QStringLiteral("type"), QStringLiteral("sessions.snapshot") },
        { QStringLiteral("sessions"), entries } };
}
}
