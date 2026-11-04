#pragma once

#include <QString>

namespace kodosi {

// An address on the loopback interface names the computer that serves it.
[[nodiscard]] bool isLocalAddress(QString host);

}
