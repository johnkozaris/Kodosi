#pragma once

#include "runtime/RuntimeEventFaults.hpp"

#include <QJsonObject>
#include <QList>
#include <QObject>
#include <QString>
#include <QVariantMap>

namespace kodosi {
class Workspace;

// The runtime asks the owner before it lets someone new into their terminals or projects, and
// words each question itself. The app shows the questions one at a time, in the order they came.
class SharingModel final : public QObject {
    Q_OBJECT
    Q_PROPERTY(QVariantMap question READ question NOTIFY questionChanged FINAL)

public:
    explicit SharingModel(Workspace& workspace);

    QVariantMap question() const;
    Q_INVOKABLE void answer(const QString& approvalId, bool allow);

signals:
    void questionChanged();

private:
    friend class Workspace;

    struct Question {
        QString approvalId;
        QString title;
        QString message;
        QString consent;
        QString confirm;
        QString asker;
        qint64 readSeconds = 0;
    };

    Workspace& m_workspace;
    QList<Question> m_questions;
    RuntimeEventFaults m_faults;

    void apply(const QString& type, const QJsonObject& event);
    void forget(const QString& approvalId);
    void reset();
};
}
