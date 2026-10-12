#include "logging/QtLogForwarder.hpp"

#include <QByteArray>
#include <QString>
#include <QtLogging>

#include <kodosi_runtime.h>

#include <atomic>
#include <optional>

namespace kodosi {
namespace {

std::atomic<QtMessageHandler> previous = nullptr;

std::optional<int32_t> runtimeLevel(const QtMsgType type)
{
    switch (type) {
    case QtDebugMsg:
        return std::nullopt;
    case QtInfoMsg:
        return KODOSI_LOG_INFO;
    case QtWarningMsg:
        return KODOSI_LOG_WARNING;
    case QtCriticalMsg:
    case QtFatalMsg:
        return KODOSI_LOG_ERROR;
    }
    return KODOSI_LOG_ERROR;
}

void forward(const QtMsgType type, const QMessageLogContext& context, const QString& message)
{
    if (const auto handler = previous.load(std::memory_order_acquire))
        handler(type, context, message);
    const auto level = runtimeLevel(type);
    if (!level)
        return;
    const auto line = QByteArray(context.category ? context.category : "default") + ": " + message.toUtf8();
    kodosi_log(*level, reinterpret_cast<const uint8_t*>(line.constData()), static_cast<uintptr_t>(line.size()));
}

}

QtLogForwarder::QtLogForwarder()
{
    previous.store(qInstallMessageHandler(forward), std::memory_order_release);
}

QtLogForwarder::~QtLogForwarder()
{
    qInstallMessageHandler(previous.exchange(nullptr, std::memory_order_acq_rel));
}

}
