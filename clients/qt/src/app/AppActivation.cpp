#include "app/AppActivation.hpp"

#include <QJsonDocument>
#include <QJsonObject>
#include <QMetaObject>
#include <QScopeGuard>

#include <algorithm>
#include <array>
#include <utility>

namespace kodosi {
namespace {

const auto tokenKey = QStringLiteral("token");
const auto sessionKey = QStringLiteral("session");
const auto routeErrorKey = QStringLiteral("routeError");
const auto appVersionKey = QStringLiteral("appVersion");
constexpr qsizetype maximumWaiting = 16;
constexpr std::array forwardedErrors {
    DeepLinkParseError::Oversized,
    DeepLinkParseError::UnsupportedRoute,
    DeepLinkParseError::MalformedEncoding,
    DeepLinkParseError::UnsafeText,
    DeepLinkParseError::InvalidQuery,
};

std::optional<DeepLinkParseError> routeError(const QJsonValue& value)
{
    const auto code = value.toInt(-1);
    const auto found = std::ranges::find_if(forwardedErrors, [code](const DeepLinkParseError error) {
        return static_cast<int>(error) == code;
    });
    return value.isDouble() && found != forwardedErrors.end() ? std::optional(*found) : std::nullopt;
}

}

QByteArray AppActivation::toJson() const
{
    QJsonObject object;
    if (!token.isEmpty())
        object.insert(tokenKey, token);
    if (route && route->destination)
        object.insert(sessionKey, route->destination->sessionId);
    else if (route)
        object.insert(routeErrorKey, static_cast<int>(route->error));
    return QJsonDocument(object).toJson(QJsonDocument::Compact);
}

AppActivation AppActivation::fromJson(const QByteArray& json)
{
    AppActivation activation;
    const auto object = QJsonDocument::fromJson(json).object();
    if (const auto token = object.value(tokenKey).toString(); isSafeActivationToken(token))
        activation.token = token;
    const auto version = object.value(appVersionKey).toString();
    if (version.size() <= 32 && std::ranges::all_of(version, [](const QChar character) {
            return character.isLetterOrNumber() || character == QLatin1Char('.') || character == QLatin1Char('-')
                || character == QLatin1Char('+');
        }))
        activation.appVersion = version;
    const auto session = object.value(sessionKey).toString();
    if (DeepLinkRouter::isSafeIdentity(session))
        activation.route = DeepLinkParseResult { .destination = DeepLinkDestination { session }, .error = {} };
    else if (const auto error = routeError(object.value(routeErrorKey)))
        activation.route = DeepLinkParseResult { .destination = std::nullopt, .error = *error };
    return activation;
}

void AppActivations::receive(const uint8_t* json, const uintptr_t length, void* activations)
{
    auto* self = static_cast<AppActivations*>(activations);
    auto activation = AppActivation::fromJson(
        QByteArray(reinterpret_cast<const char*>(json), static_cast<qsizetype>(length)));
    QMetaObject::invokeMethod(
        self, [self, activation = std::move(activation)]() mutable { self->deliver(std::move(activation)); },
        Qt::QueuedConnection);
}

void AppActivations::setHandler(std::function<void(const AppActivation&)> handler)
{
    m_handler = std::move(handler);
    for (const auto& activation : std::exchange(m_waiting, {}))
        m_handler(activation);
}

void AppActivations::deliver(AppActivation activation)
{
    if (m_handler)
        m_handler(activation);
    else if (m_waiting.size() < maximumWaiting)
        m_waiting.append(std::move(activation));
}

bool isSafeActivationToken(const QStringView token)
{
    return token.toUtf8().size() <= MaximumActivationTokenBytes
        && std::none_of(token.cbegin(), token.cend(), [](const QChar character) {
               return character.isNull() || character.category() == QChar::Other_Control;
           });
}

QString takeActivationToken()
{
    const auto token = qEnvironmentVariable("XDG_ACTIVATION_TOKEN");
    qunsetenv("XDG_ACTIVATION_TOKEN");
    return isSafeActivationToken(token) ? token : QString();
}

void withActivationToken(const QString& token, const std::function<void()>& action)
{
    if (!action)
        return;
    if (token.isEmpty() || !isSafeActivationToken(token) || !qputenv("XDG_ACTIVATION_TOKEN", token.toUtf8())) {
        action();
        return;
    }
    const auto clear = qScopeGuard([] { qunsetenv("XDG_ACTIVATION_TOKEN"); });
    action();
}

}
