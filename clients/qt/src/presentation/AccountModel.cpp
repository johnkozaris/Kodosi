#include "presentation/AccountModel.hpp"
#include "presentation/Workspace.hpp"

namespace kodosi {
AccountModel::AccountModel(Workspace& workspace)
    : m_workspace(workspace)
{
}

void AccountModel::login()
{
    m_workspace.sendUntracked(QStringLiteral("auth.login.start"));
}

void AccountModel::cancelLogin()
{
    m_workspace.sendUntracked(QStringLiteral("auth.login.cancel"));
}

void AccountModel::deleteAccount()
{
    m_workspace.sendUntracked(QStringLiteral("auth.deleteAccount"));
}

void AccountModel::cancelDeletion()
{
    endDeletion();
    m_workspace.sendUntracked(QStringLiteral("auth.deleteAccount.cancel"));
}

void AccountModel::endDeletion()
{
    if (!m_deleting && m_deletionUri.isEmpty())
        return;
    m_deleting = false;
    m_deletionUri.clear();
    emit deletionChanged();
}

void AccountModel::logout()
{
    m_workspace.sendUntracked(QStringLiteral("auth.logout"));
}

void AccountModel::reset()
{
    m_userId.clear();
    m_displayName.clear();
    m_userCode.clear();
    m_verificationUri.clear();
    m_epoch.reset();
    m_finalizing = true;
    endDeletion();
    emit accountChanged();
    emit loginChanged();
}
}
