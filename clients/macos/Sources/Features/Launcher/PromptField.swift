import AppKit
import KodosiDesign
import SwiftUI

/// The words a person types after the command: the agent's first prompt, or
/// the command that a plain terminal runs. It shows a terminal's block cursor
/// and, while it is empty, a faint suggestion that Tab or → takes.
struct PromptField: NSViewRepresentable {
    @Environment(\.theme) private var theme
    @Binding var text: String
    /// The suggestion as far as it is typed now, and in full.
    let ghost: String
    let suggestion: String
    let label: String
    /// Each new value puts the keyboard focus in the field.
    var focus = 0
    let submit: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeNSView(context: Context) -> PromptTextView {
        let view = PromptTextView(usingTextLayoutManager: false)
        view.delegate = context.coordinator
        view.isRichText = false
        view.importsGraphics = false
        view.drawsBackground = false
        view.isHorizontallyResizable = false
        view.isVerticallyResizable = false
        view.textContainerInset = NSSize(width: 0, height: 4.5)
        view.textContainer?.lineFragmentPadding = 0
        view.textContainer?.widthTracksTextView = false
        view.allowsUndo = true
        view.isAutomaticQuoteSubstitutionEnabled = false
        view.isAutomaticDashSubstitutionEnabled = false
        view.isAutomaticTextReplacementEnabled = false
        view.isAutomaticSpellingCorrectionEnabled = false
        view.isContinuousSpellCheckingEnabled = false
        view.isGrammarCheckingEnabled = false
        view.smartInsertDeleteEnabled = false
        view.insertionPointColor = .clear
        view.font = PromptTextView.font
        view.typingAttributes = [.font: PromptTextView.font]
        return view
    }

    func updateNSView(_ view: PromptTextView, context: Context) {
        context.coordinator.parent = self
        if view.string != text {
            view.string = text
        }
        view.textColor = NSColor(theme.colors.ink)
        view.selectedTextAttributes = [.backgroundColor: NSColor(theme.colors.accent.opacity(0.32))]
        view.caretColor = NSColor(theme.colors.accent)
        view.ghostColor = NSColor(theme.colors.inkFaint)
        view.inverseColor = NSColor(theme.colors.terminal)
        view.blinks = !theme.motion.reduced
        view.ghost = text.isEmpty ? ghost : ""
        view.setAccessibilityLabel(label)
        view.setAccessibilityPlaceholderValue(suggestion)
        if context.coordinator.focus != focus {
            context.coordinator.focus = focus
            DispatchQueue.main.async { view.window?.makeFirstResponder(view) }
        }
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView view: PromptTextView, context _: Context) -> CGSize? {
        let width = proposal.width ?? 260
        return CGSize(width: width, height: view.height(forWidth: width))
    }

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: PromptField
        var focus = 0

        init(_ parent: PromptField) {
            self.parent = parent
        }

        func textDidChange(_ notification: Notification) {
            guard let view = notification.object as? PromptTextView else { return }
            // One line: a pasted line break or tab becomes a space.
            let line = view.string.replacing(/[\r\n\t\u{2028}\u{2029}]+/, with: " ")
            if line != view.string {
                view.string = line
            }
            parent.text = line
            view.wake()
        }

        func textView(_ view: NSTextView, doCommandBy selector: Selector) -> Bool {
            switch selector {
            case #selector(NSResponder.insertNewline(_:)), #selector(NSResponder.insertNewlineIgnoringFieldEditor(_:)):
                parent.submit()
            case #selector(NSResponder.insertTab(_:)), #selector(NSResponder.moveRight(_:)):
                if view.string.isEmpty, !parent.suggestion.isEmpty {
                    view.insertText(parent.suggestion, replacementRange: view.selectedRange())
                } else if selector == #selector(NSResponder.insertTab(_:)) {
                    view.window?.selectNextKeyView(nil)
                } else {
                    return false
                }
            case #selector(NSResponder.insertBacktab(_:)):
                view.window?.selectPreviousKeyView(nil)
            case #selector(NSResponder.cancelOperation(_:)):
                guard !view.string.isEmpty else { return false }
                view.insertText("", replacementRange: NSRange(location: 0, length: (view.string as NSString).length))
            default:
                return false
            }
            return true
        }
    }
}

/// A one-line text view that draws a block cursor, as a terminal does: lit
/// while the person types, then blinking; hollow when the view has no focus.
final class PromptTextView: NSTextView {
    static let font = NSFont.monospacedSystemFont(ofSize: 13, weight: .medium)

    var caretColor = NSColor.systemOrange
    var ghostColor = NSColor.secondaryLabelColor
    var inverseColor = NSColor.black
    var blinks = true
    var ghost = "" {
        didSet {
            if ghost != oldValue {
                needsDisplay = true
            }
        }
    }

    private var lit = true
    private var blinking: Task<Void, Never>?

    private var cell: CGSize {
        CGSize(width: ("M" as NSString).size(withAttributes: [.font: Self.font]).width,
               height: layoutManager?.defaultLineHeight(for: Self.font) ?? 17)
    }

    private var focused: Bool {
        window?.firstResponder === self
    }

    /// The height that the text needs at `width`, with room for the cursor
    /// at the end of each line. The inset centers the first line on a key.
    func height(forWidth width: CGFloat) -> CGFloat {
        let inset = textContainerInset.height * 2
        guard let layoutManager, let textContainer else { return cell.height + inset }
        textContainer.containerSize = NSSize(width: max(cell.width, width - cell.width), height: .greatestFiniteMagnitude)
        layoutManager.ensureLayout(for: textContainer)
        return max(cell.height, ceil(layoutManager.usedRect(for: textContainer).height)) + inset
    }

    override func setFrameSize(_ size: NSSize) {
        super.setFrameSize(size)
        textContainer?.containerSize = NSSize(width: max(cell.width, size.width - cell.width), height: .greatestFiniteMagnitude)
    }

    /// Lights the cursor and starts its blink again after a pause.
    func wake() {
        lit = true
        needsDisplay = true
        blinking?.cancel()
        guard blinks, focused else { return }
        blinking = Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(1100))
            while !Task.isCancelled, let self {
                lit.toggle()
                needsDisplay = true
                try? await Task.sleep(for: .milliseconds(lit ? 560 : 440))
            }
        }
    }

    override func becomeFirstResponder() -> Bool {
        let accepted = super.becomeFirstResponder()
        wake()
        return accepted
    }

    override func resignFirstResponder() -> Bool {
        let resigned = super.resignFirstResponder()
        blinking?.cancel()
        needsDisplay = true
        return resigned
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil {
            blinking?.cancel()
        }
    }

    override func setSelectedRanges(_ ranges: [NSValue], affinity: NSSelectionAffinity, stillSelecting: Bool) {
        super.setSelectedRanges(ranges, affinity: affinity, stillSelecting: stillSelecting)
        wake()
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        let origin = textContainerOrigin
        if string.isEmpty, !ghost.isEmpty {
            (ghost as NSString).draw(at: origin, withAttributes: [.font: Self.font, .foregroundColor: ghostColor])
        }
        guard selectedRange().length == 0 else { return }
        let caret = caretRect().offsetBy(dx: origin.x, dy: origin.y)
        let block = NSBezierPath(roundedRect: caret.insetBy(dx: 0.5, dy: 1), xRadius: 1.5, yRadius: 1.5)
        guard focused else {
            caretColor.withAlphaComponent(0.85).setStroke()
            let outline = NSBezierPath(roundedRect: caret.insetBy(dx: 1.25, dy: 1.75), xRadius: 1.5, yRadius: 1.5)
            outline.lineWidth = 1.5
            outline.stroke()
            return
        }
        guard lit else { return }
        caretColor.setFill()
        block.fill()
        // The character under the cursor shows in the terminal's own color.
        let text = string as NSString
        let location = selectedRange().location
        let under = location < text.length ? text.substring(with: text.rangeOfComposedCharacterSequence(at: location))
            : string.isEmpty ? ghost.first.map(String.init) ?? "" : ""
        (under as NSString).draw(at: caret.origin, withAttributes: [.font: Self.font, .foregroundColor: inverseColor])
    }

    /// The cell of the insertion point, in text container coordinates.
    private func caretRect() -> NSRect {
        let cell = cell
        guard let layoutManager, let textContainer, layoutManager.numberOfGlyphs > 0 else {
            return NSRect(origin: .zero, size: cell)
        }
        let location = selectedRange().location
        if location < (string as NSString).length {
            let glyph = layoutManager.glyphIndexForCharacter(at: location)
            let bounds = layoutManager.boundingRect(forGlyphRange: NSRange(location: glyph, length: 1), in: textContainer)
            return NSRect(x: bounds.minX, y: bounds.minY, width: cell.width, height: bounds.height)
        }
        let last = layoutManager.numberOfGlyphs - 1
        let line = layoutManager.lineFragmentRect(forGlyphAt: last, effectiveRange: nil)
        let bounds = layoutManager.boundingRect(forGlyphRange: NSRange(location: last, length: 1), in: textContainer)
        return NSRect(x: bounds.maxX, y: line.minY, width: cell.width, height: line.height)
    }
}
