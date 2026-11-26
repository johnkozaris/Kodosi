import Foundation

/// What a speech recognizer reports for a stretch of speech: its words, whether they are final,
/// and where the speech starts and ends, in seconds of the recording.
public struct TalkResult: Equatable, Sendable {
    public let words: String
    public let final: Bool
    public let start: Double
    public let end: Double

    public init(words: String, final: Bool, start: Double, end: Double) {
        self.words = words
        self.final = final
        self.start = start
        self.end = end
    }
}

/// The words of one spoken message: phrases that the recognizer settled, then the tentative tail
/// that it may still change. Times are seconds into the recording.
public struct TalkWords: Equatable, Sendable {
    public private(set) var settled = ""
    public private(set) var tentative = ""
    /// Speech before this time went out in an earlier message.
    private var sentThrough = -Double.infinity
    /// How far a result may start before the end of the speech that went out and still be new.
    private static let overlap = 0.05
    private var settledThrough = -Double.infinity
    private var heardThrough = -Double.infinity

    init() {}

    public var text: String {
        Self.joined(settled, tentative)
    }

    public var isEmpty: Bool {
        text.allSatisfy(\.isWhitespace)
    }

    /// Takes one result, and returns whether the spoken words changed; punctuation and capitals that
    /// the recognizer adds when it settles them do not count. A result that starts in speech that
    /// already went out repeats words of that message, so it is left out; the recognizer reports
    /// the new speech in it again once it settles the old.
    @discardableResult
    mutating func hear(_ result: TalkResult) -> Bool {
        guard result.start >= sentThrough - Self.overlap else { return false }
        let before = Self.spoken(text)
        // A message starts without the space or the full stop that ends the one before it.
        var words = settled.isEmpty ? String(result.words.drop { $0.isWhitespace || $0.closesPhrase }) : result.words
        if result.final {
            // A recognizer can settle speech as punctuation alone; the words that it heard stay.
            if Self.spoken(words).isEmpty {
                words = tentative
            }
            settled = Self.joined(settled, words)
            tentative = ""
            settledThrough = max(settledThrough, result.end)
        } else {
            tentative = words
        }
        heardThrough = max(heardThrough, result.end)
        return Self.spoken(text) != before
    }

    /// Returns the message and starts the next one with the speech after these words. Words that
    /// went out while tentative end where the recognizer last heard speech, settled words where
    /// it settled them.
    mutating func take() -> String {
        let message = text.trimmingCharacters(in: .whitespacesAndNewlines)
        sentThrough = tentative.isEmpty ? settledThrough : heardThrough
        settled = ""
        tentative = ""
        return message
    }

    private static func spoken(_ text: String) -> String {
        String(text.lowercased().filter { $0.isLetter || $0.isNumber })
    }

    /// Joins two runs of words with a space, except in scripts that write words without spaces.
    public static func joined(_ left: String, _ right: String) -> String {
        guard let last = left.last else { return right }
        guard let first = right.first else { return left }
        if last.isWhitespace || first.isWhitespace || first.closesPhrase || last.writesWithoutSpaces
            || first.writesWithoutSpaces
        {
            return left + right
        }
        return left + " " + right
    }
}

private extension Character {
    /// Punctuation that ends a phrase or sits in it, such as . , ) and ”. Marks that open a phrase,
    /// such as ( “ and the Spanish ¿ ¡, belong to the words after them.
    var closesPhrase: Bool {
        guard let category = unicodeScalars.first?.properties.generalCategory else { return false }
        return switch category {
        case .connectorPunctuation, .dashPunctuation, .closePunctuation, .finalPunctuation: true
        case .otherPunctuation: self != "¿" && self != "¡"
        default: false
        }
    }

    /// Chinese characters and Japanese kana, with their punctuation.
    var writesWithoutSpaces: Bool {
        guard let scalar = unicodeScalars.first?.value else { return false }
        return switch scalar {
        case 0x3000 ... 0x30FF, 0x31F0 ... 0x31FF, 0x3400 ... 0x4DBF, 0x4E00 ... 0x9FFF, 0xF900 ... 0xFAFF,
             0xFF00 ... 0xFFEF, 0x20000 ... 0x2FA1F: true
        default: false
        }
    }
}
