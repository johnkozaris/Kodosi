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

/// A request to show the app, from this launch or a later one.
struct AppActivation {
    std::optional<DeepLinkParseResult> route;
    QString token;

    [[nodiscard]] QByteArray toJson() const;
    /// Unknown or malformed fields are dropped; the window is still shown.
    [[nodiscard]] static AppActivation fromJson(const QByteArray& json);
};

/// Receives activations that later launches hand to this app. The runtime
/// calls `receive` on its own thread; the handler runs on this object's
/// thread, and activations wait until a handler is set.
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
/// Removes this launch's XDG activation token from the environment and
/// returns it when it is safe to pass on.
[[nodiscard]] QString takeActivationToken();
/// Runs `action` with `token` as the pending XDG activation token, so the
/// window it raises may take the focus.
void withActivationToken(const QString& token, const std::function<void()>& action);

}
