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
    // An agent reports its model by its full ID, such as claude-sonnet-5-5 or
    // claude-sonnet-4-5-20250929, where the launcher knows the model by a shorter one, such as
    // sonnet: the name of the shorter one and the version after it make "Sonnet 5.5".
    [[nodiscard]] static QString modelName(const QString& model, const QVariantMap& models);
};

}
