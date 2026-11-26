#include "presentation/SharingModel.hpp"
#include "presentation/DesktopStateModel.hpp"
#include "presentation/Workspace.hpp"

#include <algorithm>
#include <utility>

namespace kodosi {
namespace {
constexpr qsizetype mostQuestions = 32;
}

SharingModel::SharingModel(Workspace& workspace)
    : m_workspace(workspace)
{
}

QVariantMap SharingModel::question() const
{
    if (m_questions.isEmpty())
        return {};
    const auto& shown = m_questions.first();
    return { { QStringLiteral("approvalId"), shown.approvalId }, { QStringLiteral("title"), shown.title },
        { QStringLiteral("message"), shown.message }, { QStringLiteral("confirm"), shown.confirm } };
}

void SharingModel::answer(const QString& approvalId, bool allow)
{
    if (std::ranges::find(m_questions, approvalId, &Question::approvalId) == m_questions.end())
        return;
    if (m_workspace.sendUntracked(QStringLiteral("sharing.answer"),
            { { QStringLiteral("approvalId"), approvalId }, { QStringLiteral("allow"), allow } }))
        forget(approvalId);
}

void SharingModel::apply(const QString& type, const QJsonObject& event)
{
    const auto id = event.value(QStringLiteral("approvalId")).toString();
    if (type == QStringLiteral("sharing.approvalEnded")) {
        forget(id);
        return;
    }
    if (std::ranges::find(m_questions, id, &Question::approvalId) != m_questions.end())
        return;
    Question question { .approvalId = id,
        .title = event.value(QStringLiteral("title")).toString(),
        .message = event.value(QStringLiteral("message")).toString(),
        .confirm = event.value(QStringLiteral("confirm")).toString() };
    const auto from = event.value(QStringLiteral("from")).toString();
    if (id.isEmpty() || question.title.isEmpty() || question.message.isEmpty() || question.confirm.isEmpty()
        || (from != QStringLiteral("app") && from != QStringLiteral("command"))) {
        m_faults.unreadable(type, QStringLiteral("its question is incomplete"));
        return;
    }
    if (m_questions.size() >= mostQuestions) {
        m_faults.unreadable(type, QStringLiteral("%1 questions wait already").arg(mostQuestions));
        return;
    }
    m_questions.append(std::move(question));
    if (m_questions.size() == 1)
        emit questionChanged();
    if (from == QStringLiteral("command"))
        m_workspace.m_desktop.askForAttention();
}

void SharingModel::forget(const QString& approvalId)
{
    const auto asked = std::ranges::find(m_questions, approvalId, &Question::approvalId);
    if (asked == m_questions.end())
        return;
    const bool shown = asked == m_questions.begin();
    m_questions.erase(asked);
    if (shown)
        emit questionChanged();
}

void SharingModel::reset()
{
    if (m_questions.isEmpty())
        return;
    m_questions.clear();
    emit questionChanged();
}
}
