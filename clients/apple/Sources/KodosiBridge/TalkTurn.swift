import Foundation

/// When a spoken message goes out. A long quiet after the person stops talking sends their words,
/// because people stop to think while they talk; Send sends them sooner. Either way they go once
/// the recognizer settled the last of them, or after a short wait for it.
public struct TalkTurn: Sendable {
    public enum Step: Equatable, Sendable {
        case wait
        case send(String)
        /// Nobody talked for a long time.
        case end
    }

    /// The quiet after the person stops talking that sends their words.
    public static let pause: TimeInterval = 3
    private static let settling: TimeInterval = 2
    private static let idle: TimeInterval = 120

    public private(set) var words = TalkWords()
    /// When the person last talked or their words last changed, while words wait to go out; the
    /// pause that sends them counts from here.
    public private(set) var quietSince: Date?
    /// When a send was asked for; the words go once the recognizer settled them.
    public private(set) var sendAsked: Date?
    /// The send was asked for while the agent asked a question, so the words answer it.
    private var sendAnswers = false
    private var voice = TalkVoice()
    private var lastWord: Date
    private var holding = false

    public init(at time: Date) {
        lastWord = time
    }

    /// Takes one result of the recognizer. Every step takes `holding`: the agent asks a question,
    /// which words would answer.
    public mutating func hear(_ result: TalkResult, at time: Date, holding: Bool) -> Step {
        hold(holding, at: time)
        if words.hear(result) {
            quietSince = time
            lastWord = time
        }
        return deliverIfSettled(at: time)
    }

    /// Takes the loudness of the latest sound from the microphone.
    public mutating func sound(_ decibels: Double, at time: Date) {
        if voice.hears(decibels, at: time), !words.isEmpty {
            quietSince = time
        }
    }

    /// The person asks to send the words now.
    public mutating func askToSend(at time: Date, holding: Bool) -> Step {
        hold(holding, at: time)
        guard !words.isEmpty, sendAsked == nil else { return .wait }
        sendAsked = time
        sendAnswers = holding
        return deliverIfSettled(at: time)
    }

    /// Moves time on: a long quiet asks to send the words, unless the agent asks a question.
    public mutating func tick(at time: Date, holding: Bool) -> Step {
        hold(holding, at: time)
        if let quietSince, !holding, sendAsked == nil, !words.isEmpty, time.timeIntervalSince(quietSince) >= Self.pause {
            sendAsked = time
            sendAnswers = false
        }
        let step = deliverIfSettled(at: time)
        if step == .wait, time.timeIntervalSince(lastWord) >= Self.idle {
            return .end
        }
        return step
    }

    /// The words that did not go out, which end this turn.
    public mutating func takeUnsent() -> String {
        quietSince = nil
        sendAsked = nil
        return words.take()
    }

    /// A question that opens stops a send that was not asked for as its answer, since words sent
    /// now would answer it; once it closes, the words wait a whole pause again.
    private mutating func hold(_ holding: Bool, at time: Date) {
        if holding, sendAsked != nil, !sendAnswers {
            sendAsked = nil
        }
        if self.holding, !holding, quietSince != nil {
            quietSince = time
        }
        self.holding = holding
    }

    private mutating func deliverIfSettled(at time: Date) -> Step {
        guard let asked = sendAsked, words.tentative.isEmpty || time.timeIntervalSince(asked) >= Self.settling else { return .wait }
        sendAsked = nil
        guard !words.isEmpty else { return .wait }
        quietSince = nil
        lastWord = time
        return .send(words.take())
    }
}
