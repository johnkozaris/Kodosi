#pragma once

#include <QtSystemDetection>

#ifdef Q_OS_WIN
#include "platform/windows/ToastNotificationDriver.hpp"
#else
#include "platform/linux/FreedesktopNotificationDriver.hpp"
#endif

namespace kodosi {

#ifdef Q_OS_WIN
using SystemNotificationDriver = ToastNotificationDriver;
#else
using SystemNotificationDriver = FreedesktopNotificationDriver;
#endif

}
