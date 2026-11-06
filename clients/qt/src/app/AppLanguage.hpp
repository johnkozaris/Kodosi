#pragma once
#include <QList>
#include <QString>

class QCoreApplication;

namespace kodosi::AppLanguage {

struct Choice {
    QString tag;
    // The name of the language in the language itself.
    QString name;
};

const QList<Choice>& choices();
// The language that the system asks for, among Kodosi's languages, or English.
QString system();
// The language of a choice in Settings; an empty choice follows the system.
QString resolved(const QString& chosen);
QString name(const QString& tag);
// Loads the words of Kodosi and of Qt in a language for this run of the app.
void install(QCoreApplication& app, const QString& tag);
// The language of this run of the app.
QString current();

}
