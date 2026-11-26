import Foundation
@testable import KodosiBridge
import Testing

private func said(_ words: String, final: Bool, from start: Double, to end: Double) -> TalkResult {
    TalkResult(words: words, final: final, start: start, end: end)
}

@Test func `words that went out while still tentative do not come back when the recognizer settles them`() {
    var words = TalkWords()
    words.hear(said("Fix the", final: true, from: 0.2, to: 1.0))
    words.hear(said("login", final: false, from: 1.0, to: 1.6))
    words.hear(said("login test", final: false, from: 1.0, to: 2.0))

    #expect(words.take() == "Fix the login test")
    #expect(words.hear(said("login test and then", final: false, from: 1.0, to: 5.2)) == false)
    #expect(words.hear(said("login test.", final: true, from: 1.0, to: 2.0)) == false)
    #expect(words.isEmpty)

    words.hear(said("Then run it", final: false, from: 2.0, to: 5.6))
    #expect(words.text == "Then run it")
}

@Test func `settling words without changing them keeps the pause, and the next message starts at its first word`() {
    var words = TalkWords()
    let heard = words.hear(said("fix the login test", final: false, from: 0, to: 2.0))
    let settled = words.hear(said("Fix the login test.", final: true, from: 0, to: 2.1))
    #expect(heard && !settled)
    #expect(words.take() == "Fix the login test.")

    words.hear(said(". Then run the tests again.", final: true, from: 6.2, to: 8.0))
    #expect(words.take() == "Then run the tests again.")

    words.hear(said("¿Y los tests?", final: true, from: 9.0, to: 10.0))
    #expect(words.text == "¿Y los tests?")
}

@Test func `speech that the recognizer settles as punctuation alone keeps the words that it heard`() {
    var words = TalkWords()
    words.hear(said("Then run the tests again", final: false, from: 6.0, to: 8.0))
    words.hear(said(".....", final: true, from: 6.7, to: 8.1))

    #expect(words.take() == "Then run the tests again")
}

@Test func `spoken phrases join with spaces except in Chinese and Japanese`() {
    #expect(TalkWords.joined("Fix the bug", "and run the tests") == "Fix the bug and run the tests")
    #expect(TalkWords.joined("Fix the bug", ".") == "Fix the bug.")
    #expect(TalkWords.joined("Hola.", "¿Qué tal?") == "Hola. ¿Qué tal?")
    #expect(TalkWords.joined("バグを", "直して") == "バグを直して")
    #expect(TalkWords.joined("修复", "这个错误") == "修复这个错误")
    #expect(TalkWords.joined("버그를", "고쳐 주세요") == "버그를 고쳐 주세요")
}

@Test func `a voice stands out from steady sound, and steady sound is no voice even when it is loud`() {
    var voice = TalkVoice()
    var time = Date()
    func sound(_ decibels: Double) -> Bool {
        time += 0.1
        return voice.hears(decibels, at: time)
    }

    #expect((0 ..< 30).map { _ in sound(-45) }.allSatisfy { !$0 })
    #expect(sound(-20))
    #expect(sound(-46) == false)

    var loud = true
    for _ in 0 ..< 200 {
        loud = sound(-20)
    }
    #expect(loud == false)
}

private let start = Date(timeIntervalSince1970: 1_800_000_000)

private func at(_ seconds: Double) -> Date {
    start + seconds
}

@Test func `a long quiet after the person stops talking sends their words, and a thinking pause does not`() {
    var turn = TalkTurn(at: at(0))
    turn.sound(-60, at: at(0))
    _ = turn.hear(said("Fix the login test", final: false, from: 0, to: 1), at: at(1.2), holding: false)
    turn.sound(-20, at: at(1.3))
    #expect(turn.tick(at: at(3.3), holding: false) == .wait)

    turn.sound(-20, at: at(3.4))
    _ = turn.hear(said("Fix the login test and keep the old error", final: false, from: 0, to: 4), at: at(4.5), holding: false)
    _ = turn.hear(said("Fix the login test and keep the old error.", final: true, from: 0, to: 4), at: at(5), holding: false)
    #expect(turn.tick(at: at(10.4), holding: false) == .wait)
    #expect(turn.tick(at: at(10.6), holding: false) == .send("Fix the login test and keep the old error."))
}

@Test func `while the agent asks a question only Send sends the words, and they wait a whole pause after it closes`() {
    var turn = TalkTurn(at: at(0))
    _ = turn.hear(said("Use the staging database", final: true, from: 0, to: 2), at: at(2), holding: false)
    #expect(turn.tick(at: at(10), holding: true) == .wait)
    #expect(turn.tick(at: at(11), holding: false) == .wait)
    #expect(turn.tick(at: at(16.9), holding: false) == .wait)
    #expect(turn.tick(at: at(17.1), holding: false) == .send("Use the staging database"))

    _ = turn.hear(said("No, use the local one", final: true, from: 18, to: 19), at: at(19), holding: false)
    _ = turn.tick(at: at(30), holding: true)
    #expect(turn.askToSend(at: at(30.1), holding: true) == .send("No, use the local one"))
}

@Test func `a send waits for the recognizer to settle the last words, but not for long`() {
    var turn = TalkTurn(at: at(0))
    _ = turn.hear(said("Run the tests", final: false, from: 0, to: 1), at: at(1), holding: false)
    #expect(turn.askToSend(at: at(1.1), holding: false) == .wait)
    #expect(turn.hear(said("Run the tests again.", final: true, from: 0, to: 1.6), at: at(1.8), holding: false) == .send("Run the tests again."))

    _ = turn.hear(said("and deploy", final: false, from: 2, to: 3), at: at(3), holding: false)
    #expect(turn.askToSend(at: at(3.1), holding: false) == .wait)
    #expect(turn.tick(at: at(5), holding: false) == .wait)
    #expect(turn.tick(at: at(5.2), holding: false) == .send("and deploy"))
}

@Test func `talking ends after two minutes without words`() {
    var turn = TalkTurn(at: at(0))
    #expect(turn.tick(at: at(119), holding: false) == .wait)
    #expect(turn.tick(at: at(121), holding: false) == .end)
}

@Test func `a question that opens while the last words settle stops their send, and Send then answers it`() {
    var turn = TalkTurn(at: at(0))
    _ = turn.hear(said("Use the staging database", final: false, from: 0, to: 1), at: at(1), holding: false)
    #expect(turn.tick(at: at(7), holding: false) == .wait)
    #expect(turn.tick(at: at(7.5), holding: true) == .wait)
    #expect(turn.hear(said("Use the staging database.", final: true, from: 0, to: 1.2), at: at(7.8), holding: true) == .wait)
    #expect(turn.tick(at: at(10), holding: true) == .wait)
    #expect(turn.askToSend(at: at(10.1), holding: true) == .send("Use the staging database."))
}
