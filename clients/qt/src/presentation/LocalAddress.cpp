#include "presentation/LocalAddress.hpp"

#include <QStringList>

#include <algorithm>

namespace kodosi {

namespace {

bool isByte(const QString& part)
{
    bool number = false;
    const auto value = part.toUInt(&number);
    return number && value <= 255;
}

}

bool isLocalAddress(QString host)
{
    host = host.toLower();
    if (host.startsWith(u'[') && host.endsWith(u']')) {
        host = host.sliced(1, host.size() - 2);
    }
    if (host == QStringLiteral("localhost") || host.endsWith(QStringLiteral(".localhost"))
        || host == QStringLiteral("0.0.0.0") || host == QStringLiteral("::") || host == QStringLiteral("::1")) {
        return true;
    }
    const auto parts = host.split(u'.');
    return parts.size() == 4 && parts.first() == QStringLiteral("127") && std::ranges::all_of(parts, isByte);
}

}
