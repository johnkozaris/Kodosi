#include "presentation/ProviderFilesModel.hpp"

#include <QJsonDocument>
#include <QUuid>

namespace kodosi {
ProviderFilesModel::ProviderFilesModel(CommandDispatcher& commands, QObject* parent)
    : QObject(parent)
    , m_commands(commands)
{
    m_timeout.setSingleShot(true);
    m_timeout.setInterval(15000);
    connect(&m_timeout, &QTimer::timeout, this, [this] {
        m_requestId.clear();
        emit busyChanged();
        fail(tr("The provider did not respond in time."));
    });
}

void ProviderFilesModel::inspect(Provider provider, const QString& directory)
{
    reset();
    m_requestId = QUuid::createUuidV7().toString(QUuid::WithoutBraces);
    QJsonObject values {
        { QStringLiteral("type"), QStringLiteral("provider.inspect") },
        { QStringLiteral("requestId"), m_requestId },
        { QStringLiteral("provider"),
            provider == Claude ? QStringLiteral("claude") : QStringLiteral("copilot") }
    };
    if (!directory.isEmpty())
        values.insert(QStringLiteral("workingDirectory"), directory);
    const auto result
        = m_commands.send(QJsonDocument(values).toJson(QJsonDocument::Compact));
    if (!result) {
        m_requestId.clear();
        fail(result.error().message);
    } else {
        m_timeout.start();
    }
    emit busyChanged();
}

void ProviderFilesModel::reset()
{
    m_timeout.stop();
    m_requestId.clear();
    m_installation.clear();
    m_error.clear();
    emit busyChanged();
    emit installationChanged();
    emit errorChanged();
}

void ProviderFilesModel::apply(const QJsonObject& event)
{
    if (event.value(QStringLiteral("requestId")).toString() != m_requestId
        || m_requestId.isEmpty()) {
        return;
    }
    const auto type = event.value(QStringLiteral("type")).toString();
    if (type != QStringLiteral("provider.reply")
        && type != QStringLiteral("provider.error")) {
        return;
    }
    m_requestId.clear();
    m_timeout.stop();
    emit busyChanged();
    if (type == QStringLiteral("provider.error")) {
        fail(event.value(QStringLiteral("message")).toString());
        return;
    }
    if (event.value(QStringLiteral("operation")).toString()
            != QStringLiteral("provider.inspect")
        || !event.value(QStringLiteral("result")).isObject()) {
        fail(tr("The provider returned an unexpected response."));
        return;
    }
    m_installation = event.value(QStringLiteral("result")).toObject().toVariantMap();
    emit installationChanged();
}

void ProviderFilesModel::fail(QString message)
{
    m_error = std::move(message);
    emit errorChanged();
}
}
