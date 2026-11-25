#pragma once

#include <QCoreApplication>
#include <QJsonObject>
#include <QList>
#include <QString>
#include <QStringList>
#include <QVariantMap>

namespace kodosi {

class ChatWords final {
    Q_DECLARE_TR_FUNCTIONS(ChatWords)
public:
    [[nodiscard]] static QString icon(const QString& kind);
    [[nodiscard]] static QString step(const QJsonObject& tool);
    [[nodiscard]] static QString summary(const QList<QJsonObject>& tools);
    [[nodiscard]] static QVariantList setbacks(const QList<QJsonObject>& tools);
    [[nodiscard]] static QString mode(const QString& mode);
    [[nodiscard]] static QString modeIcon(const QString& mode);
    [[nodiscard]] static QString duration(qint64 ms);
    [[nodiscard]] static QString cost(double dollars);
    // Formats a full model ID (e.g. "claude-sonnet-4-5-20250929") using the launcher's shorter
    // model catalog entries (e.g. "sonnet" -> "Sonnet 4.5").
    [[nodiscard]] static QString modelName(const QString& model, const QVariantMap& models);
};

}
