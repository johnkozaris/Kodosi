#if os(iOS)
import UIKit

@MainActor
final class TerminalTextInputHandler {
    private weak var view: UITerminalView?
    private var markedTextState = TerminalMarkedTextState()

    var hasMarkedText: Bool {
        markedTextState.hasMarkedText
    }

    var documentLength: Int {
        markedTextState.documentLength
    }

    init(view: UITerminalView) {
        self.view = view
    }

    func insertText(_ text: String) {
        guard let view else { return }
        TerminalDebugLog.log(
            .input,
            "insertText text=\(TerminalDebugLog.describe(text)) marked=\(hasMarkedText)"
        )
        let notifySelection = shouldNotifySelectionChange
        view.inputDelegate?.textWillChange(view)
        if notifySelection {
            view.inputDelegate?.selectionWillChange(view)
        }

        let wasComposing = markedTextState.hasMarkedText
        markedTextState.clear()
        if wasComposing {
            view.surface?.preedit("")
        }
        view.sendTypedText(text)

        if notifySelection {
            view.inputDelegate?.selectionDidChange(view)
        }
        view.inputDelegate?.textDidChange(view)
    }

    func setMarkedText(_ text: String?, selectedRange: NSRange) {
        guard let view else { return }
        TerminalDebugLog.log(
            .ime,
            "setMarkedText text=\(TerminalDebugLog.describe(text ?? "")) selected=\(selectedRange.location)+\(selectedRange.length)"
        )
        view.inputDelegate?.textWillChange(view)
        view.inputDelegate?.selectionWillChange(view)

        markedTextState.setMarkedText(text, selectedRange: selectedRange)
        view.surface?.preedit(markedTextState.text ?? "")

        view.inputDelegate?.selectionDidChange(view)
        view.inputDelegate?.textDidChange(view)
    }

    func unmarkText() {
        guard let view else { return }
        let committedText = markedTextState.text
        TerminalDebugLog.log(
            .ime,
            "unmarkText committed=\(TerminalDebugLog.describe(committedText ?? ""))"
        )
        let notifySelection = shouldNotifySelectionChange
        view.inputDelegate?.textWillChange(view)
        if notifySelection {
            view.inputDelegate?.selectionWillChange(view)
        }

        markedTextState.clear()
        view.surface?.preedit("")
        if let committedText, !committedText.isEmpty {
            view.sendTypedText(committedText)
        }

        if notifySelection {
            view.inputDelegate?.selectionDidChange(view)
        }
        view.inputDelegate?.textDidChange(view)
    }

    func deleteBackwardInMarkedText() -> Bool {
        guard let view, markedTextState.hasMarkedText else { return false }
        view.inputDelegate?.textWillChange(view)
        view.inputDelegate?.selectionWillChange(view)

        _ = markedTextState.deleteBackward()
        view.surface?.preedit(markedTextState.text ?? "")

        view.inputDelegate?.selectionDidChange(view)
        view.inputDelegate?.textDidChange(view)
        return true
    }

    func markedTextRange() -> TerminalTextRange? {
        guard markedTextState.hasMarkedText else { return nil }
        let range = markedTextState.markedRange
        return TerminalTextRange(location: range.location, length: range.length)
    }

    func selectedTextRange() -> TerminalTextRange {
        let range = markedTextState.selectedRange
        return TerminalTextRange(location: range.location, length: range.length)
    }

    func setSelectedTextRange(_ range: UITextRange?) {
        let requested = (range as? TerminalTextRange).map {
            NSRange(location: $0.location, length: $0.length)
        } ?? NSRange(location: 0, length: 0)
        let length = markedTextState.documentLength
        let location = min(max(requested.location, 0), length)
        let end = min(max(requested.location + requested.length, location), length)
        let clamped = NSRange(location: location, length: end - location)
        guard markedTextState.selectedRange != clamped else { return }
        view.map { $0.inputDelegate?.selectionWillChange($0) }
        markedTextState.setMarkedText(markedTextState.text, selectedRange: clamped)
        view.map { $0.inputDelegate?.selectionDidChange($0) }
    }

    func text(in range: TerminalTextRange) -> String? {
        markedTextState.text(in: NSRange(location: range.location, length: range.length))
    }

    func notifyGeometryDidChange() {
        guard let view, view.isFirstResponder || hasMarkedText else { return }
        view.inputDelegate?.selectionWillChange(view)
        view.inputDelegate?.selectionDidChange(view)
    }

    private var shouldNotifySelectionChange: Bool {
        hasMarkedText
            || markedTextState.selectedRange.location != 0
            || markedTextState.selectedRange.length != 0
    }
}
#endif
