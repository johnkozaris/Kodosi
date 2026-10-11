#pragma once

#include "runtime/RuntimeBridge.hpp"

#include <QJsonObject>
#include <QObject>
#include <QTimer>
#include <QVariantMap>

namespace kodosi {
class ProviderFilesModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantMap installation READ installation NOTIFY installationChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)
    Q_PROPERTY(QString error READ error NOTIFY errorChanged)

public:
    enum Provider { Claude, Copilot };
    Q_ENUM(Provider)

    explicit ProviderFilesModel(CommandDispatcher& commands, QObject* parent = nullptr);

    QVariantMap installation() const { return m_installation; }
    bool busy() const { return !m_requestId.isEmpty(); }
    QString error() const { return m_error; }

    Q_INVOKABLE void inspect(Provider provider, const QString& directory);
    Q_INVOKABLE void reset();

    void apply(const QJsonObject& event);

signals:
    void installationChanged();
    void busyChanged();
    void errorChanged();

private:
    CommandDispatcher& m_commands;
    QTimer m_timeout;
    QString m_requestId;
    QString m_error;
    QVariantMap m_installation;

    void fail(QString message);
};
}
