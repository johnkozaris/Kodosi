#pragma once

#include "runtime/RuntimeBridge.hpp"
#include "terminal/TerminalFrame.hpp"
#include "terminal/TerminalSettings.hpp"

#include <QByteArray>
#include <QString>

#include <expected>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

namespace kodosi {

enum class TerminalKey {
    Backquote,
    Backslash,
    BracketLeft,
    BracketRight,
    Comma,
    Digit,
    Equal,
    Minus,
    Period,
    Quote,
    Semicolon,
    Slash,
    Space,
    NumpadDigit,
    NumpadAdd,
    NumpadSubtract,
    NumpadMultiply,
    NumpadDivide,
    NumpadDecimal,
    NumpadEnter,
    NumpadEqual,
    Enter,
    Backspace,
    Tab,
    Escape,
    Home,
    End,
    Insert,
    Delete,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Function,
    Character,
};

enum class TerminalKeyAction {
    Press,
    Repeat,
    Release,
};

struct TerminalSelectionPointer {
    std::uint16_t column = 0;
    std::uint16_t row = 0;
    double x = 0;
    double y = 0;
};

struct TerminalSelectionPress {
    TerminalSelectionPointer pointer;
    std::uint64_t timeNs = 0;
    std::uint64_t repeatIntervalNs = 0;
    double repeatDistance = 0;
    bool output = false;
};

struct TerminalSelectionDrag {
    TerminalSelectionPointer pointer;
    std::uint32_t cellWidth = 0;
    std::uint32_t screenHeight = 0;
    bool rectangle = false;
};

struct TerminalModifiers {
    bool shift = false;
    bool control = false;
    bool alt = false;
    bool superKey = false;
};

struct TerminalKeyEvent {
    TerminalKey key;
    TerminalKeyAction action;
    QString text;
    std::uint32_t unshiftedCodepoint = 0;
    TerminalModifiers modifiers;
};

enum class TerminalMouseAction {
    Press,
    Release,
    Motion,
};

enum class TerminalMouseButton {
    None,
    Left,
    Right,
    Middle,
    WheelUp,
    WheelDown,
};

struct TerminalMouseEvent {
    TerminalMouseAction action;
    TerminalMouseButton button;
    TerminalModifiers modifiers;
    float x = 0;
    float y = 0;
    std::uint32_t screenWidth = 0;
    std::uint32_t screenHeight = 0;
    std::uint32_t cellWidth = 0;
    std::uint32_t cellHeight = 0;
    bool anyButtonPressed = false;
};

struct TerminalClipboardOperation {
    bool selection = false;
    std::optional<QString> write;
    QByteArray answerPrefix;
    QByteArray answerTerminator;
};

using TerminalClipboardOperations = std::vector<TerminalClipboardOperation>;

void appendClipboardOperation(
    TerminalClipboardOperations& operations,
    TerminalClipboardOperation operation);

class GhosttyTerminalKernel final {
public:
    struct Failure {
        enum class Code {
            MissingCheckpoint,
            StaleSubscription,
            StaleFrame,
            HitTestRace,
            SequenceMismatch,
            ResourceLimit,
            GhosttyRejected,
        };

        Code code;
        QString message;

        [[nodiscard]] bool isRecoverableHitTestRace() const noexcept
        {
            return code == Code::StaleFrame || code == Code::HitTestRace;
        }
    };

    struct HistoryMemory {
        bool compressionSupported = false;
        std::uint64_t residentBytes = 0;
        std::uint64_t compressedPages = 0;
    };

    using Frame = std::shared_ptr<const TerminalFrame>;
    using Result = std::expected<Frame, Failure>;
    using ConfigureResult = std::expected<std::optional<Frame>, Failure>;

    explicit GhosttyTerminalKernel(TerminalKernelSettings settings = {});
    ~GhosttyTerminalKernel();

    GhosttyTerminalKernel(const GhosttyTerminalKernel&) = delete;
    GhosttyTerminalKernel& operator=(const GhosttyTerminalKernel&) = delete;
    GhosttyTerminalKernel(GhosttyTerminalKernel&&) = delete;
    GhosttyTerminalKernel& operator=(GhosttyTerminalKernel&&) = delete;



    [[nodiscard]] Result installCheckpoint(const TerminalCheckpoint& checkpoint);
    [[nodiscard]] Result applyData(const TerminalData& data);
    [[nodiscard]] TerminalClipboardOperations takeClipboardOperations();
    [[nodiscard]] Result applyResize(
        const TerminalSubscription& subscription,
        std::uint64_t atSequence,
        std::uint16_t rows,
        std::uint16_t columns,
        std::uint32_t cellWidth,
        std::uint32_t cellHeight);
    [[nodiscard]] std::expected<QByteArray, Failure> encodeKey(
        const TerminalSubscription& subscription,
        TerminalKeyEvent event);
    [[nodiscard]] std::expected<QByteArray, Failure> encodePaste(
        const TerminalSubscription& subscription,
        QByteArray text);
    [[nodiscard]] std::expected<QByteArray, Failure> encodeMouse(
        const TerminalSubscription& subscription,
        TerminalMouseEvent event);
    [[nodiscard]] Result scrollViewport(
        const TerminalSubscription& subscription,
        int rows);
    [[nodiscard]] Result scrollViewportToBottom(
        const TerminalSubscription& subscription);
    [[nodiscard]] Result scrollViewportToTop(
        const TerminalSubscription& subscription);
    [[nodiscard]] Result jumpToPrompt(
        const TerminalSubscription& subscription,
        int prompts);
    [[nodiscard]] std::expected<QString, Failure> linkAt(
        const TerminalSubscription& subscription,
        std::uint64_t viewportRevision,
        std::uint16_t column,
        std::uint16_t row);
    [[nodiscard]] Result pressSelection(
        const TerminalSubscription& subscription,
        std::uint64_t viewportRevision,
        const TerminalSelectionPress& press);
    [[nodiscard]] Result dragSelection(
        const TerminalSubscription& subscription,
        std::uint64_t viewportRevision,
        const TerminalSelectionDrag& drag);
    void releaseSelection(const TerminalSubscription& subscription);
    [[nodiscard]] Result clearSelection(const TerminalSubscription& subscription);
    [[nodiscard]] Result advanceImageAnimations(const TerminalSubscription& subscription);
    [[nodiscard]] std::expected<QString, Failure> selectedText(
        const TerminalSubscription& subscription);
    [[nodiscard]] ConfigureResult configure(TerminalKernelSettings settings);
    [[nodiscard]] std::expected<std::size_t, Failure> scrollbackLimitLines() const;
    [[nodiscard]] std::expected<std::size_t, Failure> scrollbackLimitBytes() const;
    [[nodiscard]] Frame frame() const;
    [[nodiscard]] std::optional<std::uint64_t> nextSequence() const;
    [[nodiscard]] bool compressHistory();
    [[nodiscard]] HistoryMemory historyMemory() const;

private:
    class Impl;
    std::unique_ptr<Impl> m_impl;
};

}
