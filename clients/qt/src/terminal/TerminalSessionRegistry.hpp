#pragma once

#include "runtime/RuntimeBridge.hpp"
#include "terminal/GhosttyTerminalKernel.hpp"

#include <functional>
#include <memory>
#include <optional>

namespace kodosi {

struct TerminalFocusOutcome {
    bool applied;
    QString requestId;
    QString runtimeIncarnationId;
    QString reason;
};

struct TerminalResizeOutcome {
    bool applied;
    QString sessionId;
    QString requestId;
    QString expectedRuntimeIncarnationId;
    QString subscriptionId;
    std::uint64_t subscriptionGeneration;
    std::uint64_t surfaceGeneration;
    std::uint16_t columns;
    std::uint16_t rows;
    std::uint32_t widthPixels;
    std::uint32_t heightPixels;
    std::uint32_t cellWidthPixels;
    std::uint32_t cellHeightPixels;
    QString reason;
};

struct TerminalSurfaceIdentity {
    TerminalSubscription subscription;
    std::uint64_t surfaceGeneration;
};

class TerminalSessionRegistry final : public TerminalEventSink {
public:
    struct Listener {
        std::function<void(GhosttyTerminalKernel::Frame)> frameChanged;
        std::function<void(GhosttyTerminalKernel::Failure)> failed;
        std::function<void(std::int32_t)> connectionCompleted;
        std::function<void(TerminalFocusOutcome)> focusCompleted;
        std::function<void(TerminalResizeOutcome)> resizeCompleted;
        std::function<void()> closed;
    };

    struct SurfaceRegistration {
        bool requiresConnection;
        bool requiresRefresh;
    };

    TerminalSessionRegistry();
    ~TerminalSessionRegistry() override;

    TerminalSessionRegistry(const TerminalSessionRegistry&) = delete;
    TerminalSessionRegistry& operator=(const TerminalSessionRegistry&) = delete;

    [[nodiscard]] std::optional<SurfaceRegistration> registerSurface(
        TerminalSurfaceIdentity identity,
        Listener listener,
        TerminalKernelSettings settings = {});
    void cancelSurfaceRefresh(const TerminalSurfaceIdentity& identity);
    [[nodiscard]] bool unregisterSurface(const TerminalSurfaceIdentity& identity);
    [[nodiscard]] std::expected<QByteArray, GhosttyTerminalKernel::Failure> encodeKey(
        const TerminalSurfaceIdentity& identity,
        TerminalKeyEvent event);
    [[nodiscard]] std::expected<QByteArray, GhosttyTerminalKernel::Failure> encodePaste(
        const TerminalSurfaceIdentity& identity,
        QByteArray text);
    [[nodiscard]] std::expected<QByteArray, GhosttyTerminalKernel::Failure> encodeMouse(
        const TerminalSurfaceIdentity& identity,
        TerminalMouseEvent event);
    [[nodiscard]] GhosttyTerminalKernel::Result scrollViewport(
        const TerminalSurfaceIdentity& identity,
        int rows);
    [[nodiscard]] GhosttyTerminalKernel::Result scrollViewportToBottom(
        const TerminalSurfaceIdentity& identity);
    [[nodiscard]] std::expected<QString, GhosttyTerminalKernel::Failure> linkAt(
        const TerminalSurfaceIdentity& identity,
        std::uint64_t viewportRevision,
        std::uint16_t column,
        std::uint16_t row);
    [[nodiscard]] GhosttyTerminalKernel::Result beginSelection(
        const TerminalSurfaceIdentity& identity,
        std::uint64_t viewportRevision,
        std::uint16_t column,
        std::uint16_t row,
        bool rectangular = false);
    [[nodiscard]] GhosttyTerminalKernel::Result updateSelection(
        const TerminalSurfaceIdentity& identity,
        std::uint64_t viewportRevision,
        std::uint16_t column,
        std::uint16_t row);
    [[nodiscard]] GhosttyTerminalKernel::Result clearSelection(
        const TerminalSurfaceIdentity& identity);
    [[nodiscard]] std::expected<QString, GhosttyTerminalKernel::Failure> selectedText(
        const TerminalSurfaceIdentity& identity);
    [[nodiscard]] GhosttyTerminalKernel::ConfigureResult configure(
        const TerminalSurfaceIdentity& identity,
        TerminalKernelSettings settings);

    void receiveData(TerminalData data) noexcept override;
    void receiveControl(TerminalControl control) noexcept override;
    void receiveConnectResult(TerminalConnectResult result) noexcept override;
    [[nodiscard]] bool installSemanticCheckpoint(
        TerminalSemanticCheckpoint checkpoint) noexcept override;

private:
    class Impl;
    std::unique_ptr<Impl> m_impl;
};

}
