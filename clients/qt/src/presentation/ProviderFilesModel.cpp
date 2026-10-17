#include "presentation/ProviderFilesModel.hpp"

#include <QJsonDocument>
#include <QUuid>

namespace kodosi {
ProviderFilesModel::ProviderFilesModel(CommandDispatcher& commands, QObject* parent)
    : QObject(parent)
    , m_commands(commands)
{
}

void ProviderFilesModel::inspect(const QString& provider, const QString& directory)
{
    reset();
    m_requestId = QUuid::createUuidV7().toString(QUuid::WithoutBraces);
    QJsonObject values {
        { QStringLiteral("type"), QStringLiteral("provider.inspect") },
        { QStringLiteral("requestId"), m_requestId },
        { QStringLiteral("provider"), provider }
    };
    if (!directory.isEmpty())
        values.insert(QStringLiteral("workingDirectory"), directory);
    const auto result
        = m_commands.send(QJsonDocument(values).toJson(QJsonDocument::Compact));
    if (!result) {
        m_requestId.clear();
        fail(result.error().message);
    }
    emit busyChanged();
}

void ProviderFilesModel::reset()
{
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
    if (type != QStringLiteral("provider.result")
        && type != QStringLiteral("provider.error")) {
        return;
    }
    m_requestId.clear();
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
