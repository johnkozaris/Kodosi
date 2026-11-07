#pragma once
#include <QList>
#include <QString>

class QCoreApplication;

namespace kodosi::AppLanguage {

struct Choice {
    QString tag;
    QString name;
};

const QList<Choice>& choices();
QString resolved(const QString& chosen);
QString name(const QString& tag);
void install(QCoreApplication& app, const QString& tag);
QString current();

}
