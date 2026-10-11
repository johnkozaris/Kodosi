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
    emit accountChanged();
    emit loginChanged();
}
}
