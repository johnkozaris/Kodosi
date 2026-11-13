#include "presentation/PeopleModel.hpp"
#include "presentation/Workspace.hpp"

#include <QClipboard>
#include <QGuiApplication>
#include <QJsonObject>

namespace kodosi {
namespace {
constexpr qsizetype mostNoteBytes = 1000;
}

PeopleModel::PeopleModel(Workspace& workspace)
    : m_workspace(workspace)
{
}

void PeopleModel::request(const QString& username)
{
    m_workspace.sendUntracked(QStringLiteral("friends.request.send"),
        { { QStringLiteral("username"), username.trimmed() } });
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

void PeopleModel::block(const QString& username)
{
    m_workspace.sendUntracked(
        QStringLiteral("friends.block"), { { QStringLiteral("username"), username } });
}

void PeopleModel::unblock(const QString& username)
{
    m_workspace.sendUntracked(
        QStringLiteral("friends.unblock"), { { QStringLiteral("username"), username } });
}

void PeopleModel::report(const QString& username, const QString& note)
{
    if (reporting())
        return;
    m_reportRequest = m_workspace.sendRequest(QStringLiteral("friends.report"),
        { { QStringLiteral("username"), username }, { QStringLiteral("note"), note.trimmed() } });
    if (reporting())
        emit reportingChanged();
}

QString PeopleModel::clippedNote(const QString& note) const
{
    const auto bytes = note.toUtf8();
    if (bytes.size() <= mostNoteBytes)
        return note;
    auto end = mostNoteBytes;
    while (end > 0 && (static_cast<uchar>(bytes.at(end)) & 0xC0) == 0x80)
        --end;
    return QString::fromUtf8(bytes.first(end));
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
    m_inviteRequested.start();
    m_workspace.sendUntracked(QStringLiteral("friends.invite"));
}

void PeopleModel::receiveInvite(const QString& text)
{
    if (!m_inviteRequested.isValid() || m_inviteRequested.hasExpired(15000))
        return;
    m_inviteRequested.invalidate();
    if (auto* application = qobject_cast<QGuiApplication*>(QCoreApplication::instance()))
        application->clipboard()->setText(text, QClipboard::Clipboard);
    emit inviteCopied();
}

bool PeopleModel::answerReport(const QJsonObject& answer)
{
    if (!reporting() || answer.value(QStringLiteral("requestId")).toString() != m_reportRequest)
        return false;
    m_reportRequest.clear();
    emit reportingChanged();
    if (answer.value(QStringLiteral("type")).toString() == QStringLiteral("friends.error"))
        emit reportFailed(answer.value(QStringLiteral("message")).toString());
    else
        emit reported();
    return true;
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
    m_blocked.clear();
    m_inviteRequested.invalidate();
    m_reportRequest.clear();
    emit changed();
    emit reportingChanged();
}
}
