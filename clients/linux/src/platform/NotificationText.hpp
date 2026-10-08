#pragma once

#include <QString>
#include <QStringView>

namespace kodosi {

[[nodiscard]] QString notificationPlainText(
    QStringView value,
    qsizetype maximum);

}
