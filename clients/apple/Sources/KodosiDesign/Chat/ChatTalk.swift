#if os(iOS)
    import AVFoundation
    import KodosiBridge
    import os
    import Speech
    import SwiftUI

    /// Talking in a chat: the microphone's speech turns into the person's next message on this
    /// device, and `TalkTurn` decides when the message goes out.
    @MainActor
    @Observable
    final class ChatTalk {
        enum Phase: Equatable {
            case off, starting, listening
        }

        enum Trouble: Equatable {
            case microphone, unsupported, failed
        }

        private(set) var phase = Phase.off
        private(set) var turn = TalkTurn(at: Date())
        /// How loud the microphone hears the person, from 0 to 1.
        private(set) var level = 0.0
        /// Counts the messages that talking sent.
        private(set) var sent = 0
        var trouble: Trouble?

        @ObservationIgnored private weak var chat: TerminalChat?
        @ObservationIgnored private var listener: TalkListener?
        @ObservationIgnored private var opening: Task<Void, Never>?
        @ObservationIgnored private var stopping: Task<Void, Never>?
        @ObservationIgnored private var pacing: Task<Void, Never>?
        @ObservationIgnored private var attempt = 0

        /// The agent asks a question, which the words would answer, so only Send sends them.
        var holds: Bool {
            chat?.question != nil
        }

        /// Whether this device turns speech into text, once a chat found out while the app runs.
        private(set) static var available: Bool?

        static func findAvailable() async -> Bool {
            if let available {
                return available
            }
            let found = await TalkListener.available()
            available = found
            return found
        }

        func begin(in chat: TerminalChat, names: [String]) {
            guard phase == .off else { return }
            self.chat = chat
            trouble = nil
            turn = TalkTurn(at: Date())
            phase = .starting
            attempt += 1
            let attempt = attempt
            let earlier = [opening, stopping]
            // The listener of an earlier try lets go of the microphone and the audio first.
            opening = Task {
                for task in earlier {
                    await task?.value
                }
                await open(names: names, attempt: attempt)
            }
        }

        func send() {
            guard phase == .listening else { return }
            apply(turn.askToSend(at: Date(), holding: holds))
        }

        /// Stops listening and leaves the words that did not go out in the composer.
        func end() {
            guard phase != .off else { return }
            phase = .off
            opening?.cancel()
            pacing?.cancel()
            keep(turn.takeUnsent())
            level = 0
            if let listener {
                self.listener = nil
                stopping = Task { await listener.stop() }
            }
        }

        private func open(names: [String], attempt: Int) async {
            do {
                let listener = try await TalkListener.open(
                    names: names,
                    heard: { [weak self] heard in self?.hear(heard) },
                    loudness: { [weak self] decibels in self?.hearSound(decibels) },
                    ended: { [weak self] error in self?.listenerEnded(error) },
                )
                guard opens(attempt) else {
                    await listener.stop()
                    return
                }
                self.listener = listener
                turn = TalkTurn(at: Date())
                phase = .listening
                pacing = Task { await pace() }
            } catch TalkListener.Failure.microphoneDenied {
                fail(.microphone, attempt: attempt)
            } catch TalkListener.Failure.unsupported {
                fail(.unsupported, attempt: attempt)
            } catch {
                Logger.app.error("Talking could not start: \(error.localizedDescription, privacy: .public)")
                fail(.failed, attempt: attempt)
            }
        }

        private func opens(_ attempt: Int) -> Bool {
            self.attempt == attempt && phase == .starting
        }

        private func hear(_ result: TalkResult) {
            guard phase == .listening else { return }
            apply(turn.hear(result, at: Date(), holding: holds))
        }

        private func hearSound(_ decibels: Double) {
            guard phase == .listening else { return }
            level = min(max((decibels + 50) / 45, 0), 1)
            turn.sound(decibels, at: Date())
        }

        private func pace() async {
            while phase == .listening, !Task.isCancelled {
                try? await Task.sleep(for: .milliseconds(100))
                guard phase == .listening else { return }
                apply(turn.tick(at: Date(), holding: holds))
            }
        }

        private func apply(_ step: TalkTurn.Step) {
            switch step {
            case .wait:
                break
            case let .send(message):
                deliver(message)
            case .end:
                end()
            }
        }

        /// Sends the message the way the composer does, or keeps it there when the chat can no
        /// longer take it.
        private func deliver(_ message: String) {
            guard let chat, chat.canWrite else {
                keep(message)
                end()
                return
            }
            chat.draft = message
            chat.submit()
            sent += 1
        }

        private func keep(_ words: String) {
            guard let chat, !words.isEmpty else { return }
            chat.draft = TalkWords.joined(chat.draft, words)
        }

        private func listenerEnded(_ error: any Error?) {
            guard phase == .listening else { return }
            if let error, !(error is CancellationError) {
                Logger.app.error("Talking stopped: \(error.localizedDescription, privacy: .public)")
            }
            end()
        }

        private func fail(_ trouble: Trouble, attempt: Int) {
            guard opens(attempt) else { return }
            self.trouble = trouble
            end()
        }
    }

    /// Hears the microphone and turns speech into words on this device, in the person's language.
    @MainActor
    private final class TalkListener {
        enum Failure: Error {
            case microphoneDenied, unsupported, noMicrophone
        }

        /// A transcriber with the words that it reports and the hints that it takes.
        private struct Transcription {
            let module: any SpeechModule
            let results: AsyncThrowingStream<TalkResult, any Error>
            let hints: AnalysisContext?
        }

        /// How the app used audio before talking, to return to after it.
        private struct AudioUse {
            let category: AVAudioSession.Category
            let mode: AVAudioSession.Mode
            let options: AVAudioSession.CategoryOptions
            let hapticsWhileRecording: Bool
            let quietAlerts: Bool
        }

        private let analyzer: SpeechAnalyzer
        private let microphone: Microphone
        private let audio: AudioUse
        private var reading: Task<Void, Never>?
        private var observers: [any NSObjectProtocol] = []

        private init(analyzer: SpeechAnalyzer, microphone: Microphone) {
            self.analyzer = analyzer
            self.microphone = microphone
            let session = AVAudioSession.sharedInstance()
            audio = AudioUse(
                category: session.category, mode: session.mode, options: session.categoryOptions,
                hapticsWhileRecording: session.allowHapticsAndSystemSoundsDuringRecording,
                quietAlerts: session.prefersNoInterruptionsFromSystemAlerts,
            )
        }

        /// Asks for the microphone the first time, gets the language's model ready, then listens.
        /// The speech recognition permission covers sending speech to Apple's servers, so talking,
        /// whose speech stays on this device, does not ask for it.
        static func open(
            names: [String],
            heard: @escaping @MainActor (TalkResult) -> Void,
            loudness: @escaping @MainActor (Double) -> Void,
            ended: @escaping @MainActor (any Error?) -> Void,
        ) async throws -> TalkListener {
            guard await AVAudioApplication.requestRecordPermission() else { throw Failure.microphoneDenied }
            let transcription = try await transcription(names: names)
            guard await AssetInventory.status(forModules: [transcription.module]) != .unsupported else {
                throw Failure.unsupported
            }
            if let installation = try await AssetInventory.assetInstallationRequest(supporting: [transcription.module]) {
                try await installation.downloadAndInstall()
            }
            guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [transcription.module]) else {
                throw Failure.unsupported
            }
            // Talking again soon starts at once, as the model stays loaded for a while.
            let analyzer = SpeechAnalyzer(
                modules: [transcription.module], options: .init(priority: .userInitiated, modelRetention: .lingering),
            )
            if let hints = transcription.hints {
                try await analyzer.setContext(hints)
            }
            try await analyzer.prepareToAnalyze(in: format)
            let (input, feed) = AsyncStream<AnalyzerInput>.makeStream()
            let microphone = Microphone(format: format, feed: feed) { decibels in
                Task { @MainActor in loudness(decibels) }
            }
            try Task.checkCancellation()
            let listener = TalkListener(analyzer: analyzer, microphone: microphone)
            do {
                try listener.takeAudio()
                try await analyzer.start(inputSequence: input)
                try microphone.start()
            } catch {
                await listener.stop()
                throw error
            }
            listener.read(transcription.results, heard: heard, ended: ended)
            listener.watch(ended: ended)
            return listener
        }

        func stop() async {
            for observer in observers {
                NotificationCenter.default.removeObserver(observer)
            }
            observers = []
            microphone.stop()
            reading?.cancel()
            await analyzer.cancelAndFinishNow()
            let session = AVAudioSession.sharedInstance()
            try? session.setActive(false, options: .notifyOthersOnDeactivation)
            try? session.setCategory(audio.category, mode: audio.mode, options: audio.options)
            try? session.setAllowHapticsAndSystemSoundsDuringRecording(audio.hapticsWhileRecording)
            try? session.setPrefersNoInterruptionsFromSystemAlerts(audio.quietAlerts)
        }

        /// Takes the first of the person's languages that this device turns into text, with Apple's
        /// presets for live words.
        private static func transcription(names: [String]) async throws -> Transcription {
            switch await Transcriber.chosen() {
            case let .speech(locale)?:
                let transcriber = SpeechTranscriber(locale: locale, preset: .progressiveTranscription)
                let results = heardWords(transcriber.results) { TalkResult($0.text, final: $0.isFinal, range: $0.range) }
                return Transcription(module: transcriber, results: results, hints: nil)
            case let .dictation(locale)?:
                let transcriber = DictationTranscriber(locale: locale, preset: .progressiveLongDictation)
                let results = heardWords(transcriber.results) { TalkResult($0.text, final: $0.isFinal, range: $0.range) }
                let hints = AnalysisContext()
                hints.contextualStrings[.general] = Array(spokenNames(names).prefix(100))
                return Transcription(module: transcriber, results: results, hints: hints)
            case nil:
                throw Failure.unsupported
            }
        }

        /// Whether this device turns speech into text in one of the person's languages.
        static func available() async -> Bool {
            guard let chosen = await Transcriber.chosen() else { return false }
            let module = chosen.module
            guard await AssetInventory.status(forModules: [module]) != .unsupported else { return false }
            // An installed model that takes no audio can't listen, as in Simulator.
            guard await chosen.installed else { return true }
            return await SpeechAnalyzer.bestAvailableAudioFormat(compatibleWith: [module]) != nil
        }

        /// Names as people say them: "talk-in_chat" is "talk in chat".
        private static func spokenNames(_ names: [String]) -> [String] {
            var seen = Set<String>()
            return names.map { $0.replacing(/[-_.\/\\]+/, with: " ").trimmingCharacters(in: .whitespaces) }
                .filter { !$0.isEmpty && seen.insert($0).inserted }
        }

        /// Other audio pauses while the person talks, AirPods record at their best where they can, the
        /// haptic of a sent message still plays, and a call that shows as a banner interrupts only once
        /// it is answered.
        private func takeAudio() throws {
            let session = AVAudioSession.sharedInstance()
            try session.setCategory(.playAndRecord, mode: .default, options: [.allowBluetoothHFP, .bluetoothHighQualityRecording])
            try session.setAllowHapticsAndSystemSoundsDuringRecording(true)
            try session.setPrefersNoInterruptionsFromSystemAlerts(true)
            try session.setActive(true)
        }

        private func read(
            _ results: AsyncThrowingStream<TalkResult, any Error>,
            heard: @escaping @MainActor (TalkResult) -> Void,
            ended: @escaping @MainActor (any Error?) -> Void,
        ) {
            reading = Task {
                do {
                    for try await words in results {
                        heard(words)
                    }
                    ended(nil)
                } catch {
                    ended(error)
                }
            }
        }

        /// A call or a reset of the system's audio ends talking; a new headset or microphone keeps it
        /// going with that microphone.
        private func watch(ended: @escaping @MainActor (any Error?) -> Void) {
            let center = NotificationCenter.default
            observers.append(center.addObserver(forName: AVAudioSession.interruptionNotification, object: nil, queue: .main) { note in
                let began = (note.userInfo?[AVAudioSessionInterruptionTypeKey] as? UInt) == AVAudioSession.InterruptionType.began.rawValue
                MainActor.assumeIsolated {
                    if began {
                        ended(nil)
                    }
                }
            })
            observers.append(center.addObserver(forName: AVAudioSession.mediaServicesWereResetNotification, object: nil, queue: .main) { _ in
                MainActor.assumeIsolated {
                    ended(nil)
                }
            })
            observers.append(center.addObserver(forName: .AVAudioEngineConfigurationChange, object: microphone.engine, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated {
                    do {
                        try self?.microphone.restart()
                    } catch {
                        ended(error)
                    }
                }
            })
        }

        private static func heardWords<Results: AsyncSequence & Sendable>(
            _ results: Results, words: @escaping @Sendable (Results.Element) -> TalkResult,
        ) -> AsyncThrowingStream<TalkResult, any Error> where Results.Element: Sendable {
            AsyncThrowingStream { continuation in
                let task = Task {
                    do {
                        for try await result in results {
                            continuation.yield(words(result))
                        }
                        continuation.finish()
                    } catch {
                        continuation.finish(throwing: error)
                    }
                }
                continuation.onTermination = { _ in task.cancel() }
            }
        }
    }

    /// The transcriber for the first of the person's languages that this device turns into text.
    /// Apple's newer transcriber is the more accurate one but takes no word hints, so names help only
    /// in the languages that only the dictation transcriber knows.
    private enum Transcriber {
        case speech(Locale)
        case dictation(Locale)

        static func chosen() async -> Self? {
            for identifier in Locale.preferredLanguages {
                let language = Locale(identifier: identifier)
                if SpeechTranscriber.isAvailable, let locale = await SpeechTranscriber.supportedLocale(equivalentTo: language) {
                    return .speech(locale)
                }
                if let locale = await DictationTranscriber.supportedLocale(equivalentTo: language) {
                    return .dictation(locale)
                }
            }
            return nil
        }

        var module: any SpeechModule {
            switch self {
            case let .speech(locale): SpeechTranscriber(locale: locale, preset: .progressiveTranscription)
            case let .dictation(locale): DictationTranscriber(locale: locale, preset: .progressiveLongDictation)
            }
        }

        var installed: Bool {
            get async {
                switch self {
                case let .speech(locale): await SpeechTranscriber.installedLocales.contains(locale)
                case let .dictation(locale): await DictationTranscriber.installedLocales.contains(locale)
                }
            }
        }
    }

    private extension TalkResult {
        init(_ words: AttributedString, final: Bool, range: CMTimeRange) {
            self.init(words: String(words.characters), final: final, start: range.start.seconds, end: range.end.seconds)
        }
    }

    /// Feeds the microphone to the analyzer in the format that the analyzer reads.
    private final class Microphone: @unchecked Sendable {
        // Safe because the main actor starts, restarts and stops the engine, and the tap only runs
        // between its start and its removal.
        let engine = AVAudioEngine()
        private let format: AVAudioFormat
        private let feed: AsyncStream<AnalyzerInput>.Continuation
        private let loudness: @Sendable (Double) -> Void
        private var converter: AVAudioConverter?

        init(format: AVAudioFormat, feed: AsyncStream<AnalyzerInput>.Continuation, loudness: @escaping @Sendable (Double) -> Void) {
            self.format = format
            self.feed = feed
            self.loudness = loudness
        }

        func start() throws {
            let node = engine.inputNode
            let hardware = node.outputFormat(forBus: 0)
            guard hardware.sampleRate > 0, hardware.channelCount > 0 else { throw TalkListener.Failure.noMicrophone }
            converter = AVAudioConverter(from: hardware, to: format)
            // Without priming, the converted audio keeps the timing of the microphone, which the
            // recognizer's times for the words depend on.
            converter?.primeMethod = .none
            node.installTap(onBus: 0, bufferSize: 4096, format: hardware) { [weak self] buffer, _ in
                self?.take(buffer)
            }
            engine.prepare()
            try engine.start()
        }

        func restart() throws {
            engine.stop()
            engine.inputNode.removeTap(onBus: 0)
            try start()
        }

        func stop() {
            engine.stop()
            engine.inputNode.removeTap(onBus: 0)
            feed.finish()
        }

        private func take(_ buffer: AVAudioPCMBuffer) {
            loudness(Self.loudness(of: buffer))
            guard let converter, let converted = convert(buffer, with: converter) else { return }
            feed.yield(AnalyzerInput(buffer: converted))
        }

        private func convert(_ buffer: AVAudioPCMBuffer, with converter: AVAudioConverter) -> AVAudioPCMBuffer? {
            let frames = (Double(buffer.frameLength) * format.sampleRate / buffer.format.sampleRate).rounded(.up)
            guard let output = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: AVAudioFrameCount(frames) + 1) else { return nil }
            var supplied = false
            var error: NSError?
            let status = converter.convert(to: output, error: &error) { _, state in
                // The converter asks for input until it has enough; this tap buffer is all there is now.
                if supplied {
                    state.pointee = .noDataNow
                    return nil
                }
                supplied = true
                state.pointee = .haveData
                return buffer
            }
            guard status != .error, output.frameLength > 0 else { return nil }
            return output
        }

        /// Loudness in decibels below full scale.
        private static func loudness(of buffer: AVAudioPCMBuffer) -> Double {
            guard let samples = buffer.floatChannelData?[0], buffer.frameLength > 0 else { return -100 }
            let count = Int(buffer.frameLength)
            var sum: Float = 0
            for index in 0 ..< count {
                sum += samples[index] * samples[index]
            }
            return Double(20 * log10(max(sqrt(sum / Float(count)), 0.000_01)))
        }
    }
#endif
