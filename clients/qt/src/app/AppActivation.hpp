#pragma once

#include "app/DeepLinkRouter.hpp"

#include <QByteArray>
#include <QList>
#include <QObject>
#include <QString>
#include <QStringView>

#include <cstdint>
#include <functional>
#include <optional>

namespace kodosi {

struct AppActivation {
    std::optional<DeepLinkParseResult> route;
    QString token;

    [[nodiscard]] QByteArray toJson() const;
    [[nodiscard]] static AppActivation fromJson(const QByteArray& json);
};

class AppActivations final : public QObject {
public:
    using QObject::QObject;

    static void receive(const uint8_t* json, uintptr_t length, void* activations);
    void setHandler(std::function<void(const AppActivation&)> handler);

private:
    void deliver(AppActivation activation);

    std::function<void(const AppActivation&)> m_handler;
    QList<AppActivation> m_waiting;
};

inline constexpr qsizetype MaximumActivationTokenBytes = 1024;

[[nodiscard]] bool isSafeActivationToken(QStringView token);
[[nodiscard]] QString takeActivationToken();
void withActivationToken(const QString& token, const std::function<void()>& action);

}
