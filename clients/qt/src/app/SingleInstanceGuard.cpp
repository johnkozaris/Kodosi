#include "app/SingleInstanceGuard.hpp"
#include "app/RuntimeStorageBootstrap.hpp"

#include <QCryptographicHash>
#include <QDataStream>
#include <QDeadlineTimer>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QHash>
#include <QJsonDocument>
#include <QJsonObject>
#include <QLocalServer>
#include <QLocalSocket>
#include <QPointer>
#include <QQueue>
#include <QRegularExpression>
#include <QScopeGuard>
#include <QSet>
#include <QStandardPaths>
#include <QThread>
#include <QTimer>
#include <QUuid>

#include <sys/file.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/un.h>
#include <unistd.h>

#include <fcntl.h>

#include <algorithm>
#include <cerrno>
#include <limits>

namespace kodosi {
namespace {

constexpr quint32 maximumFrameBytes = 8192;
constexpr qsizetype maximumRememberedRequests = 64;
constexpr int endpointRetryIntervalMs = 10;

bool containsControl(const QStringView value)
{
    return std::any_of(value.cbegin(), value.cend(), [](const QChar character) {
        return character.isNull() || character.category() == QChar::Other_Control;
    });
}

QByteArray frame(const QJsonObject& object)
{
    const auto payload =
        QJsonDocument(object).toJson(QJsonDocument::Compact);
    QByteArray result;
    result.reserve(payload.size() + 4);
    QDataStream stream(&result, QIODevice::WriteOnly);
    stream.setByteOrder(QDataStream::BigEndian);
    stream << static_cast<quint32>(payload.size());
    result.append(payload);
    return result;
}

std::optional<QJsonObject> takeFrame(
    QByteArray& buffer,
    QString& error)
{
    if (buffer.size() < 4) {
        return std::nullopt;
    }
    const auto* bytes =
        reinterpret_cast<const unsigned char*>(buffer.constData());
    const auto length = static_cast<quint32>(bytes[0]) << 24U
        | static_cast<quint32>(bytes[1]) << 16U
        | static_cast<quint32>(bytes[2]) << 8U
        | static_cast<quint32>(bytes[3]);
    if (length == 0 || length > maximumFrameBytes) {
        error = QStringLiteral("Invalid activation frame length");
        return std::nullopt;
    }
    if (buffer.size() < static_cast<qsizetype>(length) + 4) {
        return std::nullopt;
    }
    const auto payload = buffer.mid(4, length);
    buffer.remove(0, static_cast<qsizetype>(length) + 4);
    QJsonParseError parseError;
    const auto document = QJsonDocument::fromJson(payload, &parseError);
    if (parseError.error != QJsonParseError::NoError
        || !document.isObject()) {
        error = QStringLiteral("Invalid activation frame");
        return std::nullopt;
    }
    return document.object();
}

bool ownedDirectory(const QString& path)
{
    struct stat status {};
    const auto native = QFile::encodeName(path);
    return ::lstat(native.constData(), &status) == 0
        && S_ISDIR(status.st_mode)
        && status.st_uid == ::getuid();
}

bool privateDirectory(const QString& path)
{
    struct stat status {};
    const auto native = QFile::encodeName(path);
    return ownedDirectory(path)
        && ::lstat(native.constData(), &status) == 0
        && (status.st_mode & 0077) == 0;
}

bool pathExists(const QString& path)
{
    struct stat status {};
    return ::lstat(
        QFile::encodeName(path).constData(),
        &status) == 0;
}

bool privateSocket(const QString& path)
{
    struct stat status {};
    const auto native = QFile::encodeName(path);
    return ::lstat(native.constData(), &status) == 0
        && S_ISSOCK(status.st_mode)
        && status.st_uid == ::getuid()
        && (status.st_mode & 0077) == 0;
}

bool sameUidPeer(const QLocalSocket* socket)
{
    struct ucred credential {};
    socklen_t length = sizeof(credential);
    const auto descriptor = socket->socketDescriptor();
    return descriptor >= 0
        && ::getsockopt(
               static_cast<int>(descriptor),
               SOL_SOCKET,
               SO_PEERCRED,
               &credential,
               &length)
            == 0
        && length == sizeof(credential)
        && credential.uid == ::getuid();
}

QString kindName(const ApplicationActivation::Kind kind)
{
    switch (kind) {
    case ApplicationActivation::Kind::Activate:
        return QStringLiteral("activate");
    case ApplicationActivation::Kind::Route:
        return QStringLiteral("route");
    case ApplicationActivation::Kind::InvalidRoute:
        return QStringLiteral("invalid");
    }
    return {};
}

std::optional<QJsonObject> encodeActivation(
    const ApplicationActivation& activation)
{
    if (!SingleInstanceGuard::isSafeActivationToken(
            activation.activationToken)) {
        return std::nullopt;
    }
    QJsonObject object {
        {QStringLiteral("version"), 1},
        {QStringLiteral("requestId"), activation.requestId},
        {QStringLiteral("kind"), kindName(activation.kind)},
    };
    if (activation.kind == ApplicationActivation::Kind::Route
        && activation.destination) {
        object.insert(
            QStringLiteral("sessionId"),
            activation.destination->sessionId);
    } else if (activation.kind
        == ApplicationActivation::Kind::InvalidRoute) {
        object.insert(
            QStringLiteral("parseError"),
            static_cast<int>(activation.parseError));
    }
    if (!activation.activationToken.isEmpty()) {
        object.insert(
            QStringLiteral("activationToken"),
            activation.activationToken);
    }
    return std::optional<QJsonObject> {std::move(object)};
}

std::optional<ApplicationActivation> decodeActivation(
    const QJsonObject& object)
{
    if (object.size() < 3 || object.size() > 6
        || object.value(QStringLiteral("version")).toInt(-1) != 1) {
        return std::nullopt;
    }
    const auto requestId =
        object.value(QStringLiteral("requestId")).toString();
    const QUuid requestUuid(requestId);
    if (requestUuid.isNull()
        || requestUuid.toString(QUuid::WithoutBraces) != requestId) {
        return std::nullopt;
    }
    const auto activationToken =
        object.value(QStringLiteral("activationToken")).toString();
    if ((object.contains(QStringLiteral("activationToken"))
            && activationToken.isEmpty())
        || !SingleInstanceGuard::isSafeActivationToken(
            activationToken)) {
        return std::nullopt;
    }
    const auto tokenFieldCount =
        activationToken.isEmpty() ? 0 : 1;
    const auto kind = object.value(QStringLiteral("kind")).toString();
    if (kind == QStringLiteral("activate")
        && object.size() == 3 + tokenFieldCount
        && std::ranges::all_of(
            object.keys(),
            [](const QString& key) {
                return key == QStringLiteral("version")
                    || key == QStringLiteral("requestId")
                    || key == QStringLiteral("kind")
                    || key
                        == QStringLiteral("activationToken");
            })) {
        return ApplicationActivation {
            .kind = ApplicationActivation::Kind::Activate,
            .requestId = requestId,
            .destination = std::nullopt,
            .parseError = DeepLinkParseError::None,
            .activationToken = activationToken,
        };
    }
    if (kind == QStringLiteral("invalid")
        && object.size() == 4 + tokenFieldCount
        && std::ranges::all_of(
            object.keys(),
            [](const QString& key) {
                return key == QStringLiteral("version")
                    || key == QStringLiteral("requestId")
                    || key == QStringLiteral("kind")
                    || key == QStringLiteral("parseError")
                    || key
                        == QStringLiteral("activationToken");
            })) {
        const auto rawError =
            object.value(QStringLiteral("parseError")).toInt(-1);
        if (rawError < static_cast<int>(DeepLinkParseError::Oversized)
            || rawError > static_cast<int>(DeepLinkParseError::InvalidQuery)) {
            return std::nullopt;
        }
        return ApplicationActivation {
            .kind = ApplicationActivation::Kind::InvalidRoute,
            .requestId = requestId,
            .destination = std::nullopt,
            .parseError = static_cast<DeepLinkParseError>(rawError),
            .activationToken = activationToken,
        };
    }
    if (kind != QStringLiteral("route")
        || object.size() != 4 + tokenFieldCount
        || !std::ranges::all_of(
            object.keys(),
            [](const QString& key) {
                return key == QStringLiteral("version")
                    || key == QStringLiteral("requestId")
                    || key == QStringLiteral("kind")
                    || key == QStringLiteral("sessionId")
                    || key
                        == QStringLiteral("activationToken");
            })) {
        return std::nullopt;
    }
    DeepLinkDestination destination {
        .sessionId = object.value(QStringLiteral("sessionId")).toString(),
    };
    if (!DeepLinkRouter::isSafeIdentity(destination.sessionId)) {
        return std::nullopt;
    }
    return ApplicationActivation {
        .kind = ApplicationActivation::Kind::Route,
        .requestId = requestId,
        .destination = std::move(destination),
        .parseError = DeepLinkParseError::None,
        .activationToken = activationToken,
    };
}

}

class SingleInstanceGuard::Private final {
public:
    explicit Private(SingleInstanceGuard* owner)
        : q(owner)
    {
    }

    ~Private()
    {
        releaseOwnership();
    }

    SingleInstanceGuard* q;
    std::unique_ptr<QLocalServer> server;
    QString endpoint;
    int lockDescriptor = -1;
    bool ownsLock = false;
    bool ownsEndpoint = false;
    QSet<QString> rememberedRequests;
    QQueue<QString> rememberedOrder;

    struct ClientResources {
        QByteArray buffer;
        QPointer<QTimer> timer;
    };

    QHash<QLocalSocket*, ClientResources> activeClients;

    void closeEndpoint()
    {
        if (server) {
            server->close();
            activeClients.clear();
            server.reset();
        }
        if (ownsEndpoint && !endpoint.isEmpty()) {
            QLocalServer::removeServer(endpoint);
        }
        ownsEndpoint = false;
    }

    void releaseOwnership()
    {
        closeEndpoint();
        ownsLock = false;
        if (lockDescriptor >= 0) {
            ::close(lockDescriptor);
            lockDescriptor = -1;
        }
    }

    bool remember(const QString& requestId)
    {
        if (rememberedRequests.contains(requestId)) {
            return false;
        }
        rememberedRequests.insert(requestId);
        rememberedOrder.enqueue(requestId);
        while (rememberedOrder.size() > maximumRememberedRequests) {
            rememberedRequests.remove(rememberedOrder.dequeue());
        }
        return true;
    }

    void releaseClient(QLocalSocket* socket)
    {
        const auto found = activeClients.find(socket);
        if (found == activeClients.end()) {
            return;
        }
        if (found->timer) {
            found->timer->stop();
        }
        activeClients.erase(found);
    }

    void acceptConnections()
    {
        while (server->hasPendingConnections()) {
            auto* socket = server->nextPendingConnection();
            socket->setParent(server.get());
            if (!sameUidPeer(socket)) {
                socket->abort();
                socket->deleteLater();
                continue;
            }
            if (activeClients.size()
                >= SingleInstanceGuard::MaximumActiveClients) {
                socket->abort();
                socket->deleteLater();
                continue;
            }
            auto* timer = new QTimer(socket);
            timer->setSingleShot(true);
            timer->start(3000);
            activeClients.insert(
                socket,
                ClientResources {
                    .buffer = {},
                    .timer = timer,
                });
            QObject::connect(
                timer,
                &QTimer::timeout,
                q,
                [this, socket] {
                    releaseClient(socket);
                    socket->abort();
                    socket->deleteLater();
                });
            QObject::connect(
                socket,
                &QLocalSocket::readyRead,
                socket,
                [this, socket, timer] {
                    const auto found = activeClients.find(socket);
                    if (found == activeClients.end()) {
                        socket->readAll();
                        return;
                    }
                    auto& buffer = found->buffer;
                    const auto chunk = socket->readAll();
                    if (buffer.size() + chunk.size()
                        > static_cast<qsizetype>(maximumFrameBytes) + 4) {
                        socket->write(frame({
                            {QStringLiteral("ok"), false},
                            {QStringLiteral("error"),
                             QStringLiteral("oversized")},
                        }));
                        socket->disconnectFromServer();
                        return;
                    }
                    buffer.append(chunk);
                    QString frameError;
                    auto object = takeFrame(buffer, frameError);
                    if (!frameError.isEmpty()) {
                        socket->write(frame({
                            {QStringLiteral("ok"), false},
                            {QStringLiteral("error"),
                             QStringLiteral("malformed")},
                        }));
                        socket->disconnectFromServer();
                        return;
                    }
                    if (!object) {
                        return;
                    }
                    if (!buffer.isEmpty()) {
                        socket->write(frame({
                            {QStringLiteral("ok"), false},
                            {QStringLiteral("error"),
                             QStringLiteral("multiple-frames")},
                        }));
                        socket->disconnectFromServer();
                        return;
                    }
                    const auto activation = decodeActivation(*object);
                    if (!activation) {
                        socket->write(frame({
                            {QStringLiteral("ok"), false},
                            {QStringLiteral("error"),
                             QStringLiteral("invalid-request")},
                        }));
                        socket->disconnectFromServer();
                        return;
                    }
                    const auto fresh = remember(activation->requestId);
                    socket->write(frame({
                        {QStringLiteral("ok"), true},
                        {QStringLiteral("duplicate"), !fresh},
                    }));
                    socket->flush();
                    socket->disconnectFromServer();
                    timer->stop();
                    if (!fresh) {
                        return;
                    }
                    switch (activation->kind) {
                    case ApplicationActivation::Kind::Activate:
                        emit q->activationReceived(
                            activation->activationToken);
                        break;
                    case ApplicationActivation::Kind::Route:
                        emit q->routeReceived(
                            *activation->destination,
                            activation->activationToken);
                        break;
                    case ApplicationActivation::Kind::InvalidRoute:
                        emit q->invalidRouteReceived(
                            activation->parseError,
                            activation->activationToken);
                        break;
                    }
                });
            QObject::connect(
                socket,
                &QLocalSocket::disconnected,
                q,
                [this, socket] {
                    releaseClient(socket);
                    socket->deleteLater();
                });
            QObject::connect(
                socket,
                &QLocalSocket::errorOccurred,
                q,
                [this, socket](const QLocalSocket::LocalSocketError) {
                    releaseClient(socket);
                    socket->abort();
                    socket->deleteLater();
                });
            QObject::connect(
                socket,
                &QObject::destroyed,
                q,
                [this, socket] {
                    releaseClient(socket);
                });
        }
    }

    struct ForwardAttempt {
        enum class State {
            Retry,
            Forwarded,
            Failed,
        };

        State state = State::Retry;
        QString error;
    };

    ForwardAttempt forward(
        const ApplicationActivation& activation,
        QDeadlineTimer& deadline) const
    {
        if (!pathExists(endpoint)) {
            return {
                .state = ForwardAttempt::State::Retry,
                .error = {},
            };
        }
        if (!privateSocket(endpoint)) {
            return {
                .state = ForwardAttempt::State::Failed,
                .error = QStringLiteral(
                    "The existing Kodosi activation endpoint is unsafe."),
            };
        }
        QLocalSocket socket;
        socket.connectToServer(endpoint);
        const auto connectTimeout = std::min(
            deadline.remainingTime(),
            static_cast<qint64>(endpointRetryIntervalMs));
        if (connectTimeout <= 0
            || !socket.waitForConnected(
                static_cast<int>(connectTimeout))) {
            return {
                .state = ForwardAttempt::State::Retry,
                .error = {},
            };
        }
        const auto encoded = encodeActivation(activation);
        if (!encoded) {
            return {
                .state = ForwardAttempt::State::Failed,
                .error = QStringLiteral(
                    "The activation metadata is unsafe."),
            };
        }
        const auto request = frame(*encoded);
        const auto writeTimeout = deadline.remainingTime();
        if (socket.write(request) != request.size()
            || writeTimeout <= 0
            || !socket.waitForBytesWritten(
                static_cast<int>(writeTimeout))) {
            return {
                .state = ForwardAttempt::State::Retry,
                .error = {},
            };
        }
        QByteArray response;
        while (true) {
            response.append(socket.readAll());
            if (response.size()
                > static_cast<qsizetype>(maximumFrameBytes) + 4) {
                return {
                    .state = ForwardAttempt::State::Failed,
                    .error = QStringLiteral(
                        "Kodosi returned an oversized activation response."),
                };
            }
            QString responseError;
            if (auto object = takeFrame(response, responseError)) {
                if (!response.isEmpty()
                    || !object->value(QStringLiteral("ok")).toBool()) {
                    return {
                        .state = ForwardAttempt::State::Failed,
                        .error = QStringLiteral(
                            "Kodosi rejected the forwarded link."),
                    };
                }
                return {
                    .state = ForwardAttempt::State::Forwarded,
                    .error = {},
                };
            }
            if (!responseError.isEmpty()) {
                return {
                    .state = ForwardAttempt::State::Failed,
                    .error = QStringLiteral(
                        "Kodosi returned a malformed activation response."),
                };
            }
            const auto remaining = deadline.remainingTime();
            if (remaining <= 0
                || !socket.waitForReadyRead(
                    static_cast<int>(remaining))) {
                return {
                    .state = ForwardAttempt::State::Retry,
                    .error = {},
                };
            }
        }
    }
};

SingleInstanceGuard::SingleInstanceGuard(QObject* parent)
    : QObject(parent)
    , d(std::make_unique<Private>(this))
{
    qRegisterMetaType<DeepLinkDestination>();
    qRegisterMetaType<DeepLinkParseError>();
}

SingleInstanceGuard::~SingleInstanceGuard() = default;

SingleInstanceGuard::StartResult SingleInstanceGuard::start(
    const QString& runtimeDirectory,
    const QString& endpointNamespace,
    const ApplicationActivation& activation,
    const int timeoutMs)
{
    static const QRegularExpression safeNamespace(
        QStringLiteral(R"(^[a-z0-9-]{1,48}$)"));
    if (!safeNamespace.match(endpointNamespace).hasMatch()
        || !isSafeActivationToken(activation.activationToken)
        || timeoutMs <= 0
        || d->lockDescriptor >= 0
        || !QDir::isAbsolutePath(runtimeDirectory)
        || runtimeDirectory.size() > 256) {
        return {
            .state = StartState::Failed,
            .error = QStringLiteral("The Kodosi activation endpoint is invalid."),
        };
    }
    if (pathExists(runtimeDirectory)
        && !privateDirectory(runtimeDirectory)) {
        return {
            .state = StartState::Failed,
            .error = QStringLiteral(
                "The Kodosi runtime directory is unsafe."),
        };
    }
    if (!QDir().mkpath(runtimeDirectory)
        || !ownedDirectory(runtimeDirectory)
        || ::chmod(
               QFile::encodeName(runtimeDirectory).constData(),
               0700)
            != 0
        || !privateDirectory(runtimeDirectory)) {
        return {
            .state = StartState::Failed,
            .error = QStringLiteral(
                "Kodosi could not create a private runtime directory."),
        };
    }
    d->endpoint = QDir(runtimeDirectory).filePath(
        QStringLiteral("%1.s").arg(endpointNamespace));
    const auto lockPath = QDir(runtimeDirectory).filePath(
        QStringLiteral("%1.l").arg(endpointNamespace));
    const auto nativeEndpoint = QFile::encodeName(d->endpoint);
    sockaddr_un endpointAddress {};
    if (nativeEndpoint.size()
        >= static_cast<qsizetype>(sizeof(endpointAddress.sun_path))) {
        d->endpoint.clear();
        return {
            .state = StartState::Failed,
            .error = QStringLiteral(
                "The Kodosi activation endpoint path is too long."),
        };
    }
    const auto nativeLock = QFile::encodeName(lockPath);
    d->lockDescriptor = ::open(
        nativeLock.constData(),
        O_RDWR | O_CREAT | O_CLOEXEC | O_NOFOLLOW,
        0600);
    if (d->lockDescriptor < 0) {
        return {
            .state = StartState::Failed,
            .error = QStringLiteral(
                "Kodosi could not open its private activation lock."),
        };
    }
    struct stat lockStatus {};
    if (::fstat(d->lockDescriptor, &lockStatus) != 0
        || !S_ISREG(lockStatus.st_mode)
        || lockStatus.st_uid != ::getuid()
        || ::fchmod(d->lockDescriptor, 0600) != 0) {
        d->releaseOwnership();
        return {
            .state = StartState::Failed,
            .error = QStringLiteral(
                "The Kodosi activation lock is unsafe."),
        };
    }
    QDeadlineTimer deadline(timeoutMs, Qt::PreciseTimer);
    while (!deadline.hasExpired()) {
        if (::flock(d->lockDescriptor, LOCK_EX | LOCK_NB) == 0) {
            d->ownsLock = true;
            if (pathExists(d->endpoint)
                && (!privateSocket(d->endpoint)
                    || !QLocalServer::removeServer(d->endpoint))) {
                d->releaseOwnership();
                return {
                    .state = StartState::Failed,
                    .error = QStringLiteral(
                        "A stale Kodosi activation endpoint could not be removed safely."),
                };
            }
            return {
                .state = StartState::Owner,
                .error = {},
            };
        }
        if (errno != EWOULDBLOCK
            && errno != EAGAIN
            && errno != EINTR) {
            d->releaseOwnership();
            return {
                .state = StartState::Failed,
                .error = QStringLiteral(
                    "Kodosi could not reserve its activation lock."),
            };
        }

        const auto attempt = d->forward(activation, deadline);
        if (attempt.state
            == Private::ForwardAttempt::State::Forwarded) {
            d->releaseOwnership();
            return {
                .state = StartState::Forwarded,
                .error = {},
            };
        }
        if (attempt.state
            == Private::ForwardAttempt::State::Failed) {
            d->releaseOwnership();
            return {
                .state = StartState::Failed,
                .error = attempt.error,
            };
        }
        const auto remaining = deadline.remainingTime();
        if (remaining > 0) {
            QThread::msleep(
                static_cast<unsigned long>(
                    std::min(
                        remaining,
                        static_cast<qint64>(
                            endpointRetryIntervalMs))));
        }
    }

    d->releaseOwnership();
    return {
        .state = StartState::Failed,
        .error = QStringLiteral(
            "Timed out waiting for Kodosi to become ready for activation."),
    };
}

SingleInstanceGuard::StartResult SingleInstanceGuard::publishEndpoint()
{
    if (!d->ownsLock) {
        return {
            .state = StartState::Failed,
            .error = QStringLiteral(
                "Kodosi does not own the activation lock."),
        };
    }
    if (d->ownsEndpoint) {
        return {
            .state = StartState::Owner,
            .error = {},
        };
    }
    if (pathExists(d->endpoint)
        && (!privateSocket(d->endpoint)
            || !QLocalServer::removeServer(d->endpoint))) {
        d->releaseOwnership();
        return {
            .state = StartState::Failed,
            .error = QStringLiteral(
                "A stale Kodosi activation endpoint could not be removed safely."),
        };
    }
    d->server = std::make_unique<QLocalServer>();
    d->server->setSocketOptions(QLocalServer::UserAccessOption);
    d->server->setMaxPendingConnections(
        static_cast<int>(MaximumActiveClients));
    if (!d->server->listen(d->endpoint)
        || !QFile::setPermissions(
            d->endpoint,
            QFileDevice::ReadOwner | QFileDevice::WriteOwner)) {
        d->closeEndpoint();
        QLocalServer::removeServer(d->endpoint);
        d->releaseOwnership();
        return {
            .state = StartState::Failed,
            .error = QStringLiteral(
                "Kodosi could not create its private activation endpoint."),
        };
    }
    d->ownsEndpoint = true;
    connect(
        d->server.get(),
        &QLocalServer::newConnection,
        this,
        [this] { d->acceptConnections(); });
    return {
        .state = StartState::Owner,
        .error = {},
    };
}

bool SingleInstanceGuard::isOwner() const noexcept
{
    return d->ownsLock;
}

bool SingleInstanceGuard::isReady() const noexcept
{
    return d->ownsEndpoint;
}

QString SingleInstanceGuard::endpointPath() const
{
    return d->endpoint;
}

qsizetype SingleInstanceGuard::activeClientCount() const noexcept
{
    return d->activeClients.size();
}

QString SingleInstanceGuard::standardRuntimeDirectory()
{
    const auto path = QStandardPaths::writableLocation(
        QStandardPaths::RuntimeLocation);
    const auto clean = QDir::cleanPath(path);
    if (clean == QStringLiteral("/tmp")
        || clean.startsWith(QStringLiteral("/tmp/"))
        || clean == QStringLiteral("/var/tmp")
        || clean.startsWith(QStringLiteral("/var/tmp/"))) {
        return {};
    }
    return clean;
}

SingleInstanceGuard::EndpointNamespaceResult
SingleInstanceGuard::endpointNamespace()
{
    const auto root = RuntimeStorageBootstrap::resolve();
    if (!root) {
        return {
            .value = {},
            .error = root.error(),
        };
    }
    return {
        .value = QString::fromLatin1(
            QCryptographicHash::hash(
                root->runtimeRoot.toUtf8(),
                QCryptographicHash::Sha256)
                .toHex()
                .left(16)),
        .error = {},
    };
}

ApplicationActivation SingleInstanceGuard::activation(
    const std::optional<DeepLinkParseResult>& route)
{
    ApplicationActivation result {
        .kind = ApplicationActivation::Kind::Activate,
        .requestId =
            QUuid::createUuid().toString(QUuid::WithoutBraces),
        .destination = std::nullopt,
        .parseError = DeepLinkParseError::None,
        .activationToken = {},
    };
    const auto token =
        qEnvironmentVariable("XDG_ACTIVATION_TOKEN");
    if (isSafeActivationToken(token)) {
        result.activationToken = token;
    }
    if (!route) {
        return result;
    }
    if (route->accepted()) {
        result.kind = ApplicationActivation::Kind::Route;
        result.destination = route->destination;
    } else {
        result.kind = ApplicationActivation::Kind::InvalidRoute;
        result.parseError = route->error;
    }
    return result;
}

bool SingleInstanceGuard::isSafeActivationToken(
    const QStringView token)
{
    return token.toUtf8().size()
            <= MaximumActivationTokenBytes
        && !containsControl(token);
}

void SingleInstanceGuard::withActivationToken(
    const QString& token,
    const std::function<void()>& action)
{
    if (!action) {
        return;
    }
    if (token.isEmpty()
        || !isSafeActivationToken(token)) {
        action();
        return;
    }
    if (!qputenv(
            "XDG_ACTIVATION_TOKEN",
            token.toUtf8())) {
        action();
        return;
    }
    const auto clearToken =
        qScopeGuard([] {
            qunsetenv("XDG_ACTIVATION_TOKEN");
        });
    action();
}

}
