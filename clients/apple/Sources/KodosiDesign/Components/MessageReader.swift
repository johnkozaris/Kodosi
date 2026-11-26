#if os(iOS)
    import AVFoundation
    import NaturalLanguage
    import Observation

    /// Reads a message aloud in the phone's voice for the message's language, without the signs of
    /// its Markdown. Reading another message stops the first, and the app's audio returns to how it
    /// was once the voice ends.
    @MainActor @Observable
    final class MessageReader: NSObject {
        static let shared = MessageReader()

        private(set) var reading: String?
        @ObservationIgnored private let voice = AVSpeechSynthesizer()
        @ObservationIgnored private var current: AVSpeechUtterance?
        @ObservationIgnored private var audioBefore: (AVAudioSession.Category, AVAudioSession.Mode, AVAudioSession.CategoryOptions)?

        override private init() {
            super.init()
            voice.delegate = self
        }

        func read(_ text: String) {
            let session = AVAudioSession.sharedInstance()
            if audioBefore == nil {
                audioBefore = (session.category, session.mode, session.categoryOptions)
            }
            try? session.setCategory(.playback, mode: .spokenAudio, options: .duckOthers)
            try? session.setActive(true)
            let words = ProseBlock.spoken(text)
            let utterance = AVSpeechUtterance(string: words)
            if let language = NLLanguageRecognizer.dominantLanguage(for: words) {
                utterance.voice = AVSpeechSynthesisVoice(language: language.rawValue)
            }
            current = utterance
            reading = text
            voice.stopSpeaking(at: .immediate)
            voice.speak(utterance)
        }

        func stop() {
            current = nil
            voice.stopSpeaking(at: .immediate)
            ended()
        }

        fileprivate func finished(_ utterance: ObjectIdentifier) {
            guard let current, ObjectIdentifier(current) == utterance else { return }
            self.current = nil
            ended()
        }

        private func ended() {
            reading = nil
            guard let (category, mode, options) = audioBefore else { return }
            audioBefore = nil
            let session = AVAudioSession.sharedInstance()
            try? session.setActive(false, options: .notifyOthersOnDeactivation)
            try? session.setCategory(category, mode: mode, options: options)
        }
    }

    extension MessageReader: AVSpeechSynthesizerDelegate {
        nonisolated func speechSynthesizer(_: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
            let finished = ObjectIdentifier(utterance)
            Task { @MainActor in self.finished(finished) }
        }

        nonisolated func speechSynthesizer(_: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
            let finished = ObjectIdentifier(utterance)
            Task { @MainActor in self.finished(finished) }
        }
    }
#endif
