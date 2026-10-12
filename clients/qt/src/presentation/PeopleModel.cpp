#include "presentation/PeopleModel.hpp"
#include "presentation/Workspace.hpp"

#include <QClipboard>
#include <QGuiApplication>
#include <QJsonObject>

namespace kodosi {
PeopleModel::PeopleModel(Workspace& workspace)
    : m_workspace(workspace)
{
}

void PeopleModel::request(const QString& username)
{
    m_workspace.sendUntracked(QStringLiteral("friends.request.send"),
        { { QStringLiteral("username"), username.trimmed() } }, true);
}

void PeopleModel::accept(const QString& username)
{
    m_workspace.sendUntracked(
        QStringLiteral("friends.request.accept"), { { QStringLiteral("username"), username } });
}

void PeopleModel::decline(const QString& username)
{
    m_workspace.sendUntracked(
        QStringLiteral("friends.request.reject"), { { QStringLiteral("username"), username } });
}

void PeopleModel::cancel(const QString& username)
{
    m_workspace.sendUntracked(
        QStringLiteral("friends.request.cancel"), { { QStringLiteral("username"), username } });
}

void PeopleModel::remove(const QString& username)
{
    m_workspace.sendUntracked(
        QStringLiteral("friends.remove"), { { QStringLiteral("username"), username } });
}

void PeopleModel::trust(const QString& username)
{
    m_workspace.sendUntracked(
        QStringLiteral("friends.identity.trust"), { { QStringLiteral("username"), username } });
}

void PeopleModel::verify(const QString& username, const QString& invite)
{
    m_workspace.sendUntracked(QStringLiteral("friends.verify"),
        { { QStringLiteral("username"), username }, { QStringLiteral("invite"), invite.trimmed() } });
}

void PeopleModel::copyInvite()
{
    m_workspace.sendUntracked(QStringLiteral("friends.invite"));
}

void PeopleModel::receiveInvite(const QString& text)
{
    m_invite = text;
    if (auto* application = qobject_cast<QGuiApplication*>(QCoreApplication::instance()))
        application->clipboard()->setText(text, QClipboard::Clipboard);
    emit changed();
}

QString PeopleModel::displayName(const QString& userId) const
{
    for (const auto& value : m_friends) {
        const auto person = value.toMap();
        if (person.value(QStringLiteral("userId")).toString() != userId)
            continue;
        const auto name = person.value(QStringLiteral("displayName")).toString();
        return name.isEmpty() ? person.value(QStringLiteral("handle")).toString() : name;
    }
    return {};
}

void PeopleModel::reset()
{
    m_friends.clear();
    m_incoming.clear();
    m_outgoing.clear();
    m_invite.clear();
    emit changed();
}
}
