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
        { QStringLiteral("message"), shown.message }, { QStringLiteral("consent"), shown.consent },
        { QStringLiteral("confirm"), shown.confirm }, { QStringLiteral("asker"), shown.asker },
        { QStringLiteral("readSeconds"), shown.readSeconds } };
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
    const auto asker = event.value(QStringLiteral("asker"));
    Question question { .approvalId = id,
        .title = event.value(QStringLiteral("title")).toString(),
        .message = event.value(QStringLiteral("message")).toString(),
        .consent = event.value(QStringLiteral("consent")).toString(),
        .confirm = event.value(QStringLiteral("confirm")).toString(),
        .asker = asker.toString(),
        .readSeconds = event.value(QStringLiteral("readSeconds")).toInteger(-1) };
    if (id.isEmpty() || question.title.isEmpty() || question.message.isEmpty() || question.consent.isEmpty()
        || question.confirm.isEmpty() || (!asker.isString() && !asker.isNull()) || question.readSeconds < 0) {
        m_faults.unreadable(type, QStringLiteral("its question is incomplete"));
        return;
    }
    if (m_questions.size() >= mostQuestions) {
        m_faults.unreadable(type, QStringLiteral("%1 questions wait already").arg(mostQuestions));
        return;
    }
    const bool askedOutsideTheApp = !question.asker.isEmpty();
    m_questions.append(std::move(question));
    if (m_questions.size() == 1)
        emit questionChanged();
    if (askedOutsideTheApp)
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
