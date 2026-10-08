#include "terminal/TerminalSurfaceController.hpp"

#include "terminal/TerminalView.hpp"

#include <QUuid>

#include <algorithm>
#include <limits>
#include <ranges>

namespace kodosi {

TerminalSurfaceController::TerminalSurfaceController(
    TerminalSessionRegistry& registry,
    RuntimeBridge& runtime,
    SessionCatalogModel& sessions,
    const qint64 checkpointAcquisitionTimeoutMs,
    QObject* parent)
    : QObject(parent)
    , m_registry(registry)
    , m_runtime(runtime)
    , m_sessions(sessions)
    , m_checkpointAcquisitionTimeoutMs(checkpointAcquisitionTimeoutMs)
{
    Q_ASSERT(checkpointAcquisitionTimeoutMs >= 0);
    m_attachmentRetryTimer.setSingleShot(true);
    m_attachmentRetryTimer.setInterval(250);
    connect(
        &m_attachmentRetryTimer,
        &QTimer::timeout,
        this,
        &TerminalSurfaceController::retryPendingAttachments);
    const auto reconcileBindings = [this] { reconcile(); };
    connect(
        &runtime,
        &RuntimeBridge::runningChanged,
        this,
        &TerminalSurfaceController::runtimeChanged);
    connect(
        &sessions,
        &SessionCatalogModel::authoritativeSnapshotApplied,
        this,
        &TerminalSurfaceController::catalogSnapshotApplied);
    connect(&sessions, &QAbstractItemModel::modelReset, this, reconcileBindings);
    connect(&sessions, &QAbstractItemModel::rowsInserted, this, reconcileBindings);
    connect(&sessions, &QAbstractItemModel::rowsRemoved, this, reconcileBindings);
    connect(&sessions, &QAbstractItemModel::dataChanged, this, reconcileBindings);
}

bool TerminalSurfaceController::bind(
    TerminalView* view,
    const QString& sessionId)
{
    if (view == nullptr || sessionId.isEmpty()) {
        emit attachmentRejected(
            view,
            sessionId,
            QStringLiteral("The selected terminal session is invalid."));
        return false;
    }
    const auto currentIncarnation = m_sessions.incarnationForSession(sessionId);
    const auto presentation =
        m_sessions.presentationSession(sessionId);
    if (!currentIncarnation && !m_waitingForFreshCatalog) {
        emit attachmentRejected(
            view,
            sessionId,
            QStringLiteral("The selected session is no longer available."));
        return false;
    }
    if (presentation && !presentation->canRetainPresentation) {
        emit attachmentRejected(
            view,
            sessionId,
            QStringLiteral("The selected session can no longer present a terminal."));
        return false;
    }
    const auto found = std::ranges::find_if(m_bindings, [&](const Binding& binding) {
        return binding.view == view;
    });
    if (found == m_bindings.end()) {
        m_bindings.push_back({
            .view = view,
            .sessionId = sessionId,
            .runtimeIncarnationId = currentIncarnation.value_or(QString {}),
            .subscription = {},
            .readinessConnection = {},
            .checkpointTimer = {},
            .attached = false,
        });
        view->setTerminalInteraction(presentation && presentation->canControl, presentation && presentation->canControl);
        connect(view, &QObject::destroyed, this, [this, view] {
            for (auto binding = m_bindings.begin();
                 binding != m_bindings.end();) {
                if (binding->view != view && !binding->view.isNull()) {
                    ++binding;
                    continue;
                }
                cancelCheckpointTimeout(*binding);
                disconnect(binding->readinessConnection);
                binding = m_bindings.erase(binding);
            }
        });
        m_bindings.back().readinessConnection = connect(
            view,
            &TerminalView::terminalReadyChanged,
            this,
            [this, view] {
                const auto binding = std::ranges::find_if(
                    m_bindings,
                    [&](const Binding& value) { return value.view == view; });
                if (binding == m_bindings.end()
                    || !binding->attached || binding->view == nullptr
                    || !binding->view->terminalReady()
                    || binding->checkpointTimer == nullptr) {
                    return;
                }
                cancelCheckpointTimeout(*binding);
                emit attachmentReady(
                    binding->view.data(),
                    binding->sessionId);
            });
        connect(
            view,
            &TerminalView::connectionCompleted,
            this,
            [this, view](const bool connected, const std::int32_t result) {
                const auto binding = std::ranges::find_if(
                    m_bindings,
                    [&](const Binding& value) { return value.view == view; });
                if (binding == m_bindings.end()) {
                    return;
                }
                if (connected) {
                    binding->retryPending = false;
                    if (binding->checkpointTimer == nullptr) {
                        emit attachmentReady(
                            binding->view.data(),
                            binding->sessionId);
                    }
                    return;
                }
                cancelCheckpointTimeout(*binding);
                binding->view->detach();
                binding->subscription = {};
                binding->surfaceGeneration = 0;
                binding->attached = false;
                scheduleRemoteRetry(
                    *binding,
                    QStringLiteral(
                        "The runtime rejected the terminal connection (%1).")
                        .arg(result));
            });
        connect(view, &TerminalView::terminalClosed, this, [this, view] {
            const auto binding = std::ranges::find_if(
                m_bindings,
                [&](const Binding& value) { return value.view == view; });
            if (binding != m_bindings.end()) {
                cancelCheckpointTimeout(*binding);
                binding->subscription = {};
                binding->surfaceGeneration = 0;
                binding->attached = false;
            }
        });
        tryAttach(m_bindings.back());
        return true;
    }
    if (found->sessionId == sessionId) {
        found->view->setTerminalInteraction(presentation && presentation->canControl, presentation && presentation->canControl);
        tryAttach(*found);
        return true;
    }
    cancelCheckpointTimeout(*found);
    view->detach();
    found->sessionId = sessionId;
    found->runtimeIncarnationId = currentIncarnation.value_or(QString {});
    found->subscription = {};
    found->surfaceGeneration = 0;
    found->retryPending = false;
    found->attached = false;
    found->view->setTerminalInteraction(presentation && presentation->canControl, presentation && presentation->canControl);
    tryAttach(*found);
    return true;
}

bool TerminalSurfaceController::retry(
    TerminalView* view,
    const QString& sessionId)
{
    const auto currentIncarnation =
        m_sessions.incarnationForSession(sessionId);
    const auto presentation =
        m_sessions.presentationSession(sessionId);
    if (view == nullptr || sessionId.isEmpty() || !currentIncarnation
        || !presentation || !presentation->canRetainPresentation) {
        emit attachmentRejected(
            view,
            sessionId,
            QStringLiteral("The selected session is no longer available."));
        return false;
    }
    const auto found = std::ranges::find_if(m_bindings, [&](const Binding& binding) {
        return binding.view == view;
    });
    if (found == m_bindings.end()) {
        return bind(view, sessionId);
    }
    cancelCheckpointTimeout(*found);
    view->detach();
    found->sessionId = sessionId;
    found->runtimeIncarnationId = *currentIncarnation;
    found->subscription = {};
    found->surfaceGeneration = 0;
    found->retryPending = false;
    found->attached = false;
    found->view->setTerminalInteraction(presentation->canControl, presentation->canControl);
    tryAttach(*found);
    return true;
}

void TerminalSurfaceController::detach(TerminalView* view)
{
    const auto found = std::ranges::find_if(m_bindings, [&](const Binding& binding) {
        return binding.view == view;
    });
    if (found == m_bindings.end()) {
        return;
    }
    if (found->view != nullptr) {
        cancelCheckpointTimeout(*found);
        found->view->detach();
    }
    disconnect(found->readinessConnection);
    m_bindings.erase(found);
}

void TerminalSurfaceController::runtimeChanged(const bool running)
{
    m_attachmentRetryTimer.stop();
    m_waitingForFreshCatalog = true;
    for (auto& binding : m_bindings) {
        cancelCheckpointTimeout(binding);
        if (binding.view != nullptr) {
            binding.view->detach();
        }
        binding.runtimeIncarnationId.clear();
        binding.subscription = {};
        binding.surfaceGeneration = 0;
        binding.retryPending = false;
        binding.attached = false;
    }
    Q_UNUSED(running);
}

void TerminalSurfaceController::catalogSnapshotApplied()
{
    if (!m_runtime.isRunning()) {
        return;
    }
    m_waitingForFreshCatalog = false;
    reconcile();
}

void TerminalSurfaceController::reconcile()
{
    if (!m_sessions.hasAuthoritativeSnapshot()) {
        m_waitingForFreshCatalog = true;
        for (auto& binding : m_bindings) {
            cancelCheckpointTimeout(binding);
            if (binding.view != nullptr) {
                binding.view->setTerminalInteraction(false, false);
                binding.view->detach();
            }
            binding.runtimeIncarnationId.clear();
            binding.subscription = {};
            binding.surfaceGeneration = 0;
            binding.retryPending = false;
            binding.attached = false;
        }
        return;
    }
    m_waitingForFreshCatalog = false;
    for (auto binding = m_bindings.begin(); binding != m_bindings.end();) {
        if (binding->view == nullptr) {
            binding = m_bindings.erase(binding);
            continue;
        }
        const auto currentIncarnation =
            m_sessions.incarnationForSession(binding->sessionId);
        const auto presentation =
            m_sessions.presentationSession(binding->sessionId);
        if (!currentIncarnation || !presentation
            || !presentation->canRetainPresentation) {
            cancelCheckpointTimeout(*binding);
            binding->view->detach();
            binding->subscription = {};
            binding->surfaceGeneration = 0;
            emit attachmentRejected(
                binding->view.data(),
                binding->sessionId,
                QStringLiteral("The terminal session was replaced or removed."));
            disconnect(binding->readinessConnection);
            binding = m_bindings.erase(binding);
            continue;
        }
        binding->view->setTerminalInteraction(presentation->canControl, presentation->canControl);
        const auto connected = !presentation->isRemoteConnectable || presentation->canControl;
        if (connected != binding->runtimeConnected) {
            binding->runtimeConnected = connected;
            binding->retryPending = false;
            if (!connected) {
                cancelCheckpointTimeout(*binding);
                binding->view->detach();
                binding->attached = false;
                binding->subscription = {};
            }
        }
        if (binding->runtimeIncarnationId != *currentIncarnation) {
            cancelCheckpointTimeout(*binding);
            binding->view->detach();
            binding->runtimeIncarnationId = *currentIncarnation;
            binding->subscription = {};
            binding->surfaceGeneration = 0;
            binding->retryPending = false;
            binding->attached = false;
        }
        if (!m_runtime.isRunning()) {
            cancelCheckpointTimeout(*binding);
            binding->view->detach();
            binding->subscription = {};
            binding->surfaceGeneration = 0;
            binding->attached = false;
            ++binding;
            continue;
        }
        tryAttach(*binding);
        ++binding;
    }
}

void TerminalSurfaceController::tryAttach(Binding& binding)
{
    const auto presentation = m_sessions.presentationSession(binding.sessionId);
    if (presentation && presentation->isRemoteConnectable && !presentation->canControl) {
        return;
    }
    if (binding.attached || binding.retryPending
        || binding.view == nullptr || !m_runtime.isRunning()
        || m_waitingForFreshCatalog || binding.runtimeIncarnationId.isEmpty()) {
        return;
    }
    if (m_nextSurfaceGeneration == std::numeric_limits<std::uint64_t>::max()) {
        emit attachmentRejected(
            binding.view.data(),
            binding.sessionId,
            QStringLiteral("The terminal surface generation is exhausted."));
        return;
    }
    const auto shared = std::ranges::find_if(
        m_bindings,
        [&](const Binding& candidate) {
            return &candidate != &binding && candidate.attached
                && candidate.sessionId == binding.sessionId
                && candidate.runtimeIncarnationId
                    == binding.runtimeIncarnationId
                && !candidate.subscription.subscriptionId.isEmpty();
        });
    TerminalSubscription subscription;
    const auto sharedSubscription = shared != m_bindings.end();
    if (sharedSubscription) {
        subscription = shared->subscription;
    } else {
        if (m_nextSubscriptionGeneration
            == std::numeric_limits<std::uint64_t>::max()) {
            emit attachmentRejected(
                binding.view.data(),
                binding.sessionId,
                QStringLiteral(
                    "The terminal subscription generation is exhausted."));
            return;
        }
        subscription = {
            .sessionId = binding.sessionId,
            .subscriptionId =
                QUuid::createUuidV7().toString(QUuid::WithoutBraces),
            .generation = ++m_nextSubscriptionGeneration,
        };
    }
    const auto surfaceGeneration = ++m_nextSurfaceGeneration;
    binding.attached = binding.view->attach(
        m_registry,
        m_runtime,
        subscription,
        binding.runtimeIncarnationId,
        surfaceGeneration);
    binding.subscription = binding.attached
        ? std::move(subscription)
        : TerminalSubscription {};
    binding.surfaceGeneration =
        binding.attached ? surfaceGeneration : 0;
    if (!binding.attached) {
        scheduleRemoteRetry(
            binding,
            QStringLiteral("The runtime rejected the terminal attachment."));
    } else if (sharedSubscription && !binding.view->terminalReady()) {
        armCheckpointTimeout(binding);
    }
}

void TerminalSurfaceController::scheduleRemoteRetry(
    Binding& binding,
    QString reason)
{
    const auto presentation =
        m_sessions.presentationSession(binding.sessionId);
    if (!presentation || !presentation->isRemoteConnectable) {
        emit attachmentRejected(
            binding.view.data(),
            binding.sessionId,
            std::move(reason));
        return;
    }
    binding.retryPending = true;
    if (!m_attachmentRetryTimer.isActive()) {
        m_attachmentRetryTimer.start();
    }
}

void TerminalSurfaceController::retryPendingAttachments()
{
    for (auto& binding : m_bindings) {
        if (!binding.retryPending || binding.view == nullptr) {
            continue;
        }
        binding.retryPending = false;
        tryAttach(binding);
    }
}

void TerminalSurfaceController::armCheckpointTimeout(Binding& binding)
{
    cancelCheckpointTimeout(binding);
    if (!binding.attached || binding.view == nullptr
        || binding.subscription.subscriptionId.isEmpty()
        || binding.surfaceGeneration == 0) {
        return;
    }
    auto* timer = new QTimer(this);
    timer->setSingleShot(true);
    timer->setInterval(static_cast<int>(std::clamp<qint64>(
        m_checkpointAcquisitionTimeoutMs,
        0,
        std::numeric_limits<int>::max())));
    binding.checkpointTimer = timer;
    const QPointer<TerminalView> view = binding.view;
    const auto subscription = binding.subscription;
    const auto surfaceGeneration = binding.surfaceGeneration;
    connect(timer, &QTimer::timeout, this, [this, view, subscription, surfaceGeneration, timer] {
        timer->deleteLater();
        const auto binding = std::ranges::find_if(
            m_bindings,
            [&](const Binding& candidate) {
                return candidate.view == view
                    && candidate.attached
                    && candidate.surfaceGeneration == surfaceGeneration
                    && candidate.subscription.sessionId
                        == subscription.sessionId
                    && candidate.subscription.subscriptionId
                        == subscription.subscriptionId
                    && candidate.subscription.generation
                        == subscription.generation;
            });
        if (binding == m_bindings.end() || binding->view == nullptr
            || binding->view->terminalReady()) {
            return;
        }
        binding->checkpointTimer = nullptr;
        binding->view->detach();
        binding->subscription = {};
        binding->surfaceGeneration = 0;
        binding->retryPending = false;
        binding->attached = false;
        emit attachmentRejected(
            binding->view.data(),
            binding->sessionId,
            QStringLiteral(
                "The terminal checkpoint did not arrive in time."));
    });
    timer->start();
}

void TerminalSurfaceController::cancelCheckpointTimeout(Binding& binding)
{
    if (binding.checkpointTimer == nullptr) {
        return;
    }
    binding.checkpointTimer->stop();
    binding.checkpointTimer->deleteLater();
    binding.checkpointTimer = nullptr;
}

}
