#if os(iOS)
import GhosttyKit
import UIKit

extension UITerminalView: UITextInput {
    open var autocorrectionType: UITextAutocorrectionType {
        get { .no }
        set {}
    }

    open var autocapitalizationType: UITextAutocapitalizationType {
        get { .none }
        set {}
    }

    open var smartQuotesType: UITextSmartQuotesType {
        get { .no }
        set {}
    }

    open var smartDashesType: UITextSmartDashesType {
        get { .no }
        set {}
    }

    open var smartInsertDeleteType: UITextSmartInsertDeleteType {
        get { .no }
        set {}
    }

    open var spellCheckingType: UITextSpellCheckingType {
        get { .no }
        set {}
    }

    open var inlinePredictionType: UITextInlinePredictionType {
        get { .no }
        set {}
    }

    open var writingToolsBehavior: UIWritingToolsBehavior {
        get { .none }
        set {}
    }

    open var keyboardType: UIKeyboardType {
        get { .default }
        set {}
    }

    open var hasText: Bool {
        true
    }

    open func insertText(_ text: String) {
        guard !hardwareKeyHandled else {
            hardwareKeyHandled = false
            return
        }
        inputHandler.insertText(text)
    }

    open func deleteBackward() {
        if inputHandler.deleteBackwardInMarkedText() {
            hardwareKeyHandled = false
            return
        }
        guard !hardwareKeyHandled else {
            hardwareKeyHandled = false
            return
        }
        sendKeyPress(.keyboardDeleteOrBackspace)
    }

    func sendTypedText(_ text: String) {
        for stroke in TerminalKeyStroke.typed(text) {
            send(stroke, action: GHOSTTY_ACTION_PRESS)
            send(stroke, action: GHOSTTY_ACTION_RELEASE)
        }
    }

    open func setMarkedText(_ markedText: String?, selectedRange: NSRange) {
        guard !hardwareKeyHandled else {
            hardwareKeyHandled = false
            return
        }
        inputHandler.setMarkedText(markedText, selectedRange: selectedRange)
    }

    open func unmarkText() {
        inputHandler.unmarkText()
    }

    open var markedTextRange: UITextRange? {
        inputHandler.markedTextRange()
    }

    open var markedTextStyle: [NSAttributedString.Key: Any]? {
        get { nil }
        set {}
    }

    open var selectedTextRange: UITextRange? {
        get { inputHandler.selectedTextRange() }
        set { inputHandler.setSelectedTextRange(newValue) }
    }

    open var beginningOfDocument: UITextPosition {
        TerminalTextPosition(0)
    }

    open var endOfDocument: UITextPosition {
        TerminalTextPosition(inputHandler.documentLength)
    }

    open var inputDelegate: (any UITextInputDelegate)? {
        get { textInputDelegate }
        set { textInputDelegate = newValue }
    }

    open var tokenizer: any UITextInputTokenizer {
        UITextInputStringTokenizer(textInput: self)
    }

    open var textInputView: UIView {
        self
    }

    open func text(in range: UITextRange) -> String? {
        guard let range = range as? TerminalTextRange else { return nil }
        return inputHandler.text(in: range)
    }

    open func replace(_: UITextRange, withText text: String) {
        inputHandler.insertText(text)
    }

    open func textRange(
        from fromPosition: UITextPosition,
        to toPosition: UITextPosition
    ) -> UITextRange? {
        guard let from = fromPosition as? TerminalTextPosition,
              let to = toPosition as? TerminalTextPosition
        else { return nil }
        return TerminalTextRange(start: from, end: to)
    }

    open func position(
        from position: UITextPosition,
        offset: Int
    ) -> UITextPosition? {
        guard let position = position as? TerminalTextPosition else { return nil }
        let index = position.index + offset
        guard index >= 0, index <= inputHandler.documentLength else { return nil }
        return TerminalTextPosition(index)
    }

    open func position(
        from position: UITextPosition,
        in _: UITextLayoutDirection,
        offset: Int
    ) -> UITextPosition? {
        self.position(from: position, offset: offset)
    }

    open func compare(
        _ position: UITextPosition,
        to other: UITextPosition
    ) -> ComparisonResult {
        guard let lhs = position as? TerminalTextPosition,
              let rhs = other as? TerminalTextPosition
        else { return .orderedSame }
        if lhs.index < rhs.index { return .orderedAscending }
        if lhs.index > rhs.index { return .orderedDescending }
        return .orderedSame
    }

    open func offset(
        from: UITextPosition,
        to toPosition: UITextPosition
    ) -> Int {
        guard let from = from as? TerminalTextPosition,
              let to = toPosition as? TerminalTextPosition
        else { return 0 }
        return to.index - from.index
    }

    open func position(
        within range: UITextRange,
        farthestIn direction: UITextLayoutDirection
    ) -> UITextPosition? {
        switch direction {
        case .left, .up:
            range.start
        case .right, .down:
            range.end
        @unknown default:
            range.start
        }
    }

    open func characterRange(
        byExtending position: UITextPosition,
        in _: UITextLayoutDirection
    ) -> UITextRange? {
        guard inputHandler.documentLength > 0,
              let position = position as? TerminalTextPosition
        else {
            return TerminalTextRange(location: 0, length: 0)
        }
        let location = min(max(position.index, 0), inputHandler.documentLength - 1)
        return TerminalTextRange(location: location, length: 1)
    }

    open func baseWritingDirection(
        for _: UITextPosition,
        in _: UITextStorageDirection
    ) -> NSWritingDirection {
        .leftToRight
    }

    open func setBaseWritingDirection(
        _: NSWritingDirection,
        for _: UITextRange
    ) {}

    open func firstRect(for range: UITextRange) -> CGRect {
        let cell = cursorCellRect()
        guard let range = range as? TerminalTextRange else { return cell }
        let location = min(max(range.location, 0), max(inputHandler.documentLength, 1))
        return CGRect(
            x: cell.minX + CGFloat(location) * cell.width,
            y: cell.minY,
            width: max(CGFloat(range.length) * cell.width, cell.width),
            height: cell.height
        )
    }

    open func caretRect(for position: UITextPosition) -> CGRect {
        let cell = cursorCellRect()
        let index = (position as? TerminalTextPosition).map {
            min(max($0.index, 0), inputHandler.documentLength)
        } ?? inputHandler.documentLength
        return CGRect(
            x: cell.minX + CGFloat(index) * cell.width,
            y: cell.minY,
            width: 2,
            height: cell.height
        )
    }

    open func selectionRects(for _: UITextRange) -> [UITextSelectionRect] {
        []
    }

    open func closestPosition(to point: CGPoint) -> UITextPosition? {
        TerminalTextPosition(textIndex(at: point))
    }

    open func closestPosition(
        to point: CGPoint,
        within _: UITextRange
    ) -> UITextPosition? {
        closestPosition(to: point)
    }

    open func characterRange(at point: CGPoint) -> UITextRange? {
        TerminalTextRange(location: textIndex(at: point), length: 0)
    }

    private func cursorCellRect() -> CGRect {
        guard let surface else { return .zero }
        let point = surface.imePoint()
        let cellWidth: CGFloat = if let size = surface.size(), size.cellWidthPixels > 0 {
            CGFloat(size.cellWidthPixels) / displayScale
        } else {
            max(point.height / 2, 1)
        }
        return CGRect(
            x: point.x - cellWidth / 2,
            y: point.y - point.height,
            width: cellWidth,
            height: point.height
        )
    }

    private func textIndex(at point: CGPoint) -> Int {
        let cell = cursorCellRect()
        guard inputHandler.documentLength > 0, cell.width > 0 else { return 0 }
        let index = Int(((point.x - cell.minX) / cell.width).rounded(.down))
        return min(max(index, 0), inputHandler.documentLength)
    }
}
#endif
