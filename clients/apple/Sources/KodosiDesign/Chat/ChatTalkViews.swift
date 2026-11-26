#if os(iOS)
    import KodosiBridge
    import SwiftUI
    import UIKit

    /// The composer, or while a person talks, their words and the talk bar in its place, with what
    /// went wrong the last time talking tried to start. Like queued messages, words that the agent
    /// did not get yet stay down here; the conversation shows what it got.
    struct ChatTalkArea<Composer: View>: View {
        let talk: ChatTalk
        let chat: TerminalChat
        let type: () -> Void
        @ContentBuilder let composer: Composer

        var body: some View {
            if let trouble = talk.trouble {
                ChatTalkTrouble(trouble: trouble) { talk.trouble = nil }
                    .transition(AnyTransition.rise)
            }
            if talk.phase == .off {
                composer.transition(.opacity)
            } else {
                if !talk.turn.words.isEmpty {
                    ChatTalkBubble(words: talk.turn.words, edit: type)
                        .transition(AnyTransition.rise)
                }
                ChatTalkBar(talk: talk, chat: chat, type: type)
                    .transition(.opacity)
            }
        }
    }

    extension View {
        /// Talking stops when the chat can no longer take messages, leaves the screen or the app
        /// goes to the background; `sent` follows each message that talking sends.
        func talking(_ talk: ChatTalk, in chat: TerminalChat, sent: @escaping () -> Void) -> some View {
            modifier(ChatTalkLife(talk: talk, chat: chat, sent: sent))
        }
    }

    private struct ChatTalkLife: ViewModifier {
        @Environment(\.theme) private var theme
        @Environment(\.scenePhase) private var scenePhase
        let talk: ChatTalk
        let chat: TerminalChat
        let sent: () -> Void

        func body(content: Content) -> some View {
            content
                .animation(theme.motion.snappy, value: talk.phase == .off)
                .animation(theme.motion.spring, value: talk.turn.words.isEmpty)
                .animation(theme.motion.fade, value: talk.trouble)
                .onChange(of: talk.sent) { _, _ in sent() }
                .onChange(of: chat.canWrite) { _, canWrite in
                    if !canWrite {
                        talk.end()
                    }
                }
                .onChange(of: scenePhase) { _, phase in
                    if phase == .background {
                        talk.end()
                    }
                }
                .onDisappear { talk.end() }
        }
    }

    /// The microphone key of the composer, which starts talking.
    struct TalkKey: View {
        @Environment(\.theme) private var theme
        let enabled: Bool
        let action: () -> Void

        var body: some View {
            Button(action: action) {
                Image(systemName: "mic.fill").font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(enabled ? theme.colors.ink : theme.colors.inkFaint)
                    .frame(width: 30, height: 30)
                    .background { BezelBackground(shape: Circle(), fill: theme.buttonFace(), bezel: .quiet) }
                    .touchTarget()
            }
            .buttonStyle(PressScaleStyle())
            .disabled(!enabled)
            .accessibilityLabel(Text("Talk", bundle: .module))
            // VoiceOver stays quiet once talking starts, so the microphone does not hear it.
            .accessibilityAddTraits(.startsMediaSession)
            .accessibilityIdentifier("chat.talk")
        }
    }

    /// Takes the place of the composer while a person talks: the way back to typing, the bars that
    /// show that the microphone hears them, and Send, whose ring fills during the pause that sends.
    private struct ChatTalkBar: View {
        @Environment(\.theme) private var theme
        let talk: ChatTalk
        let chat: TerminalChat
        let type: () -> Void

        var body: some View {
            let heard = !talk.turn.words.isEmpty
            HStack(spacing: 8) {
                IconButton(title: "Type instead", bundle: .module, symbol: "keyboard", identifier: "chat.talk.type", size: 30,
                           action: type)
                Spacer(minLength: 0)
                HStack(spacing: 8) {
                    TalkLevel(level: talk.level, live: talk.phase == .listening)
                    if heard, talk.holds {
                        Text("Tap Send to answer", bundle: .module)
                            .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
                            .transition(.opacity)
                    }
                }
                Spacer(minLength: 0)
                if !heard, chat.working {
                    StopKey { chat.stop() }
                        .accessibilityIdentifier("chat.stop")
                        .transition(AnyTransition.pop)
                } else {
                    TalkSendKey(enabled: heard && chat.canWrite, waiting: talk.turn.sendAsked != nil,
                                quietSince: talk.holds ? nil : talk.turn.quietSince, action: talk.send)
                        .transition(AnyTransition.pop)
                }
            }
            .padding(.vertical, 1)
            .composerField(focused: true)
            .animation(theme.motion.snappy, value: heard)
            .animation(theme.motion.fade, value: talk.holds)
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("chat.talk.bar")
        }
    }

    /// The words of the message so far, where the person's next message will be: settled words
    /// in ink, words that the recognizer may still change faint.
    private struct ChatTalkBubble: View {
        @Environment(\.theme) private var theme
        let words: TalkWords
        let edit: () -> Void

        var body: some View {
            HStack(spacing: 0) {
                Spacer(minLength: 56)
                ViewThatFits(in: .vertical) {
                    spoken
                    ScrollView { spoken }.defaultScrollAnchor(.bottom).scrollIndicators(.hidden)
                }
                .frame(maxHeight: 168)
                .projectBubble()
                .contentShape(Rectangle())
                .onTapGesture(perform: edit)
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(Text("Your message, not sent yet", bundle: .module))
            .accessibilityValue(Text(verbatim: words.text))
            .accessibilityAddTraits(.isButton)
            .accessibilityAction(.default, edit)
            .accessibilityIdentifier("chat.talk.words")
        }

        private var spoken: some View {
            var text = AttributedString(words.settled)
            text.foregroundColor = theme.colors.ink
            var tail = AttributedString(String(words.text.dropFirst(words.settled.count)))
            tail.foregroundColor = theme.colors.inkFaint
            text.append(tail)
            return Text(text).appTextStyle(.body).lineSpacing(2)
        }
    }

    /// Bars that rise with how loud the microphone hears the person, flat and faint until it listens.
    private struct TalkLevel: View {
        @Environment(\.theme) private var theme
        let level: Double
        let live: Bool
        private static let profile: [Double] = [0.55, 0.85, 1, 0.75, 0.5]

        var body: some View {
            HStack(spacing: 2) {
                ForEach(Self.profile.indices, id: \.self) { index in
                    Capsule().fill(live ? theme.colors.ink : theme.colors.inkFaint)
                        .frame(width: 2.5, height: 3 + 13 * (live ? level : 0) * Self.profile[index])
                }
            }
            .frame(width: 22, height: 18)
            .animation(theme.motion.snappy, value: level)
            .animation(theme.motion.fade, value: live)
            .accessibilityElement()
            .accessibilityLabel(live ? Text("Listening", bundle: .module) : Text("Getting ready", bundle: .module))
            .accessibilityIdentifier("chat.talk.level")
        }
    }

    /// The send key of talking. Its ring fills at the pace of the clock during the pause that sends
    /// the words, and shows full at once when motion is reduced.
    private struct TalkSendKey: View {
        @Environment(\.theme) private var theme
        let enabled: Bool
        /// The words wait for the recognizer to settle the last of them before they go.
        let waiting: Bool
        /// When the person last talked, while a pause can send their words.
        let quietSince: Date?
        let action: () -> Void
        @State private var filled = 0.0
        /// A breath does not start the ring.
        private static let ringDelay: TimeInterval = 0.8

        private var countingFrom: Date? {
            enabled && !waiting ? quietSince : nil
        }

        var body: some View {
            SendKey(enabled: enabled, working: waiting, action: action)
                .overlay {
                    Circle().trim(from: 0, to: filled)
                        .stroke(theme.colors.accent, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                        .rotationEffect(.degrees(-90))
                        .padding(-4)
                        .allowsHitTesting(false)
                        .accessibilityHidden(true)
                }
                .task(id: countingFrom) { await fill(from: countingFrom) }
                .accessibilityLabel(Text("Send", bundle: .module))
                .accessibilityIdentifier("chat.talk.send")
        }

        private func fill(from quietSince: Date?) async {
            var instant = Transaction()
            instant.disablesAnimations = true
            withTransaction(instant) { filled = 0 }
            guard let quietSince else { return }
            let delay = quietSince.addingTimeInterval(Self.ringDelay).timeIntervalSinceNow
            if delay > 0 {
                try? await Task.sleep(for: .seconds(delay))
            }
            guard !Task.isCancelled else { return }
            let left = quietSince.addingTimeInterval(TalkTurn.pause).timeIntervalSinceNow
            if theme.motion.reduced || left <= 0 {
                withTransaction(instant) { filled = 1 }
            } else {
                withAnimation(.linear(duration: left)) { filled = 1 }
            }
        }
    }

    /// Why talking stopped or could not start, with the one step that fixes it.
    private struct ChatTalkTrouble: View {
        let trouble: ChatTalk.Trouble
        let dismiss: () -> Void

        var body: some View {
            CautionNote(message: message, identifier: "chat.talk.trouble", dismiss: dismiss) {
                if trouble == .microphone {
                    Button {
                        if let settings = URL(string: UIApplication.openSettingsURLString) {
                            UIApplication.shared.open(settings)
                        }
                        dismiss()
                    } label: {
                        Text("Open Settings", bundle: .module)
                    }
                    .buttonStyle(.kodosi(.tinted, size: .small))
                    .accessibilityIdentifier("chat.talk.settings")
                }
            }
        }

        private var message: String {
            switch trouble {
            case .microphone: String(localized: "Allow the microphone for Kodosi in Settings.", bundle: .module)
            case .unsupported: String(localized: "This device can't turn your speech into text.", bundle: .module)
            case .failed: String(localized: "Kodosi couldn't start listening.", bundle: .module)
            }
        }
    }
#endif
