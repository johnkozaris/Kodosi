#pragma once

#include <QString>
#include <QUrl>

#include <optional>

namespace kodosi {

[[nodiscard]] std::optional<QUrl> validatedTerminalLink(const QString& value);

}
