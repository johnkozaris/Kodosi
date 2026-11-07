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
    struct Phrase {
        QString alone;
        QString about;

        [[nodiscard]] QString operator()(const QString& place) const
        {
            return place.isEmpty() ? alone : QString(about).arg(place);
        }
    };

    struct Deed {
        Phrase done;
        Phrase doing;
        QString icon;
        Phrase said {};
        Phrase saying {};
    };

    [[nodiscard]] static Deed deed(const QString& action);
    [[nodiscard]] static QString target(const QVariantMap& where);
    [[nodiscard]] static QString receipt(const QVariantMap& tool);
    [[nodiscard]] static QString said(const QVariantMap& tool);
    [[nodiscard]] static QString summary(const QVariantList& tools);
    [[nodiscard]] static QString day(const QDateTime& date, const QDateTime& now);
    [[nodiscard]] static QString mood(const QString& name);
};

}
