#if os(iOS)
import UIKit

final class TerminalTextPosition: UITextPosition {
    let index: Int

    init(_ index: Int) {
        self.index = index
        super.init()
    }
}

final class TerminalTextRange: UITextRange {
    private let startPosition: TerminalTextPosition
    private let endPosition: TerminalTextPosition

    override var start: UITextPosition {
        startPosition
    }

    override var end: UITextPosition {
        endPosition
    }

    override var isEmpty: Bool {
        startPosition.index >= endPosition.index
    }

    var location: Int {
        startPosition.index
    }

    var length: Int {
        endPosition.index - startPosition.index
    }

    init(start: TerminalTextPosition, end: TerminalTextPosition) {
        startPosition = start
        endPosition = end
        super.init()
    }

    convenience init(location: Int, length: Int) {
        self.init(
            start: TerminalTextPosition(location),
            end: TerminalTextPosition(location + length)
        )
    }
}
#endif
