#pragma once

#include "app/DeepLinkRouter.hpp"

#include <QObject>
#include <QString>
#include <QStringView>

#include <functional>
#include <memory>
#include <optional>

class QLocalServer;

namespace kodosi {

struct ApplicationActivation {
    enum class Kind {
        Activate,
        Route,
        InvalidRoute,
    };

    Kind kind = Kind::Activate;
    QString requestId;
    std::optional<DeepLinkDestination> destination;
    DeepLinkParseError parseError = DeepLinkParseError::None;
    QString activationToken;
};

class SingleInstanceGuard final : public QObject {
    Q_OBJECT

public:
    static constexpr qsizetype MaximumActiveClients = 16;
    static constexpr qsizetype MaximumActivationTokenBytes = 1024;

    enum class StartState {
        Owner,
        Forwarded,
        Failed,
    };

    struct StartResult {
        StartState state;
        QString error;
    };

    struct EndpointNamespaceResult {
        QString value;
        QString error;

        [[nodiscard]] bool valid() const noexcept
        {
            return !value.isEmpty() && error.isEmpty();
        }
    };

    explicit SingleInstanceGuard(QObject* parent = nullptr);
    ~SingleInstanceGuard() override;

    [[nodiscard]] StartResult start(
        const QString& runtimeDirectory,
        const QString& endpointNamespace,
        const ApplicationActivation& activation,
        int timeoutMs = 3000);
    [[nodiscard]] StartResult publishEndpoint();
    [[nodiscard]] bool isOwner() const noexcept;
    [[nodiscard]] bool isReady() const noexcept;
    [[nodiscard]] QString endpointPath() const;
    [[nodiscard]] qsizetype activeClientCount() const noexcept;

    [[nodiscard]] static QString standardRuntimeDirectory();
    [[nodiscard]] static EndpointNamespaceResult endpointNamespace();
    [[nodiscard]] static ApplicationActivation activation(
        const std::optional<DeepLinkParseResult>& route);
    [[nodiscard]] static bool isSafeActivationToken(QStringView token);
    static void withActivationToken(
        const QString& token,
        const std::function<void()>& action);

signals:
    void activationReceived(QString activationToken);
    void routeReceived(
        kodosi::DeepLinkDestination destination,
        QString activationToken);
    void invalidRouteReceived(
        kodosi::DeepLinkParseError error,
        QString activationToken);

private:
    class Private;
    std::unique_ptr<Private> d;
};

}

Q_DECLARE_METATYPE(kodosi::DeepLinkDestination)
Q_DECLARE_METATYPE(kodosi::DeepLinkParseError)
