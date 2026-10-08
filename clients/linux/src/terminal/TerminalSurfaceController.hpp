#pragma once

#include "runtime/RuntimeBridge.hpp"
#include "presentation/SessionCatalogModel.hpp"
#include "terminal/TerminalSessionRegistry.hpp"

#include <QMetaObject>
#include <QObject>
#include <QPointer>
#include <QTimer>
#include <QVector>

#include <cstdint>

namespace kodosi {

class TerminalView;

class TerminalSurfaceController final : public QObject {
    Q_OBJECT

public:
    TerminalSurfaceController(
        TerminalSessionRegistry& registry,
        RuntimeBridge& runtime,
        SessionCatalogModel& sessions,
        qint64 checkpointAcquisitionTimeoutMs = 15'000,
        QObject* parent = nullptr);

    Q_INVOKABLE [[nodiscard]] bool bind(
        kodosi::TerminalView* view,
        const QString& sessionId);
    Q_INVOKABLE [[nodiscard]] bool retry(
        kodosi::TerminalView* view,
        const QString& sessionId);
    Q_INVOKABLE void detach(kodosi::TerminalView* view);

signals:
    void attachmentRejected(
        kodosi::TerminalView* surface,
        QString sessionId,
        QString reason);
    void attachmentReady(
        kodosi::TerminalView* surface,
        QString sessionId);

private:
    struct Binding {
        QPointer<TerminalView> view;
        QString sessionId;
        QString runtimeIncarnationId;
        TerminalSubscription subscription;
        QMetaObject::Connection readinessConnection;
        QPointer<QTimer> checkpointTimer;
        std::uint64_t surfaceGeneration = 0;
        bool retryPending = false;
        bool attached = false;
        bool runtimeConnected = false;
    };

    TerminalSessionRegistry& m_registry;
    RuntimeBridge& m_runtime;
    SessionCatalogModel& m_sessions;
    QVector<Binding> m_bindings;
    std::uint64_t m_nextSubscriptionGeneration = 0;
    std::uint64_t m_nextSurfaceGeneration = 0;
    QTimer m_attachmentRetryTimer;
    qint64 m_checkpointAcquisitionTimeoutMs;
    bool m_waitingForFreshCatalog = true;

    void runtimeChanged(bool running);
    void catalogSnapshotApplied();
    void reconcile();
    void tryAttach(Binding& binding);
    void scheduleRemoteRetry(Binding& binding, QString reason);
    void retryPendingAttachments();
    void armCheckpointTimeout(Binding& binding);
    void cancelCheckpointTimeout(Binding& binding);

};

}
