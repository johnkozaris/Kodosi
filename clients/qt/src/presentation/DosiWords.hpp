#pragma once

#include <QCoreApplication>
#include <QDateTime>
#include <QString>
#include <QVariantList>
#include <QVariantMap>

namespace kodosi {

class DosiWords final {
    Q_DECLARE_TR_FUNCTIONS(DosiWords)
public:
    struct Deed {
        QString done;
        QString doing;
        QString icon;
        QString saying;
        QString said;
    };

    [[nodiscard]] static Deed deed(const QString& action);
    [[nodiscard]] static QString target(const QVariantMap& where);
    [[nodiscard]] static QString doing(const QVariantMap& tool);
    [[nodiscard]] static QString receipt(const QVariantMap& tool);
    [[nodiscard]] static QString said(const QVariantMap& tool);
    [[nodiscard]] static QString summary(const QVariantList& tools);
    [[nodiscard]] static QString day(const QDateTime& date, const QDateTime& now);
    [[nodiscard]] static QString mood(const QString& name);
};

}
