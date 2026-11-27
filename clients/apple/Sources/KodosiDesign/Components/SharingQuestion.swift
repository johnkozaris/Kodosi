import KodosiBridge
import SwiftUI
#if os(iOS)
    import UIKit
#else
    import AppKit
#endif

/// A change that lets someone new in, asked as the runtime words it. The owner accepts twice: the
/// consent, then the confirm button, which works only once the question showed for its read time
/// while Kodosi was in front.
public struct SharingQuestionPanel: View {
    @Environment(\.theme) private var theme
    let approval: SharingApproval
    /// Something above the question hides it, as the app lock does.
    let covered: Bool
    let answer: (Bool) -> Void
    @State private var consented = false
    @State private var readSince: Date?
    @State private var read = false
    @State private var arrived = false
    @State private var heard = false
    /// A click or a key meant for what was there a moment ago does not answer a new question.
    @State private var settled = false
    @State private var inFront = Self.appInFront

    public init(approval: SharingApproval, covered: Bool = false, answer: @escaping (Bool) -> Void) {
        self.approval = approval
        self.covered = covered
        self.answer = answer
    }

    private var seen: Bool {
        inFront && !covered
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            CautionMark(arrived: arrived)
            Text(approval.title).appTextStyle(.title).foregroundStyle(theme.colors.ink)
                .fixedSize(horizontal: false, vertical: true)
                .accessibilityAddTraits(.isHeader)
            if let asker = approval.asker {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Image(systemName: "terminal").font(.system(size: theme.scaled(12), weight: .semibold))
                    Text(asker).appTextStyle(.footnote).fontWeight(.semibold)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .foregroundStyle(theme.colors.caution)
            }
            Text(approval.message).appTextStyle(.body).foregroundStyle(theme.colors.ink)
                .fixedSize(horizontal: false, vertical: true)
            Toggle(isOn: $consented) { Text(approval.consent) }
                .toggleStyle(ConsentStyle())
                .accessibilityIdentifier("sharing.question.consent")
            HStack(spacing: 8) {
                Spacer(minLength: 0)
                Button {
                    if settled {
                        answer(false)
                    }
                } label: { Text("Cancel", bundle: .module) }
                    .buttonStyle(.kodosi(.secondary))
                    .keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier("sharing.question.cancel")
                Button { answer(true) } label: {
                    HStack(spacing: 7) {
                        Text(approval.confirm)
                        if !read {
                            ReadRing(since: readSince, seconds: approval.readSeconds)
                                .transition(.scale(scale: 0.4).combined(with: .opacity))
                        }
                    }
                }
                .buttonStyle(.kodosi(.danger))
                .disabled(!consented || !read)
                .accessibilityIdentifier("sharing.question.confirm")
            }
        }
        .padding(24)
        #if os(iOS)
            .frame(maxWidth: Self.width, alignment: .leading)
        #else
            .frame(width: Self.width, alignment: .leading)
        #endif
            .background {
                RaisedBackground(
                    shape: RoundedRectangle(cornerRadius: Radius.sheet, style: .continuous),
                    fill: theme.colors.surface, elevation: .floating,
                )
            }
            .accessibilityElement(children: .contain)
            .accessibilityIdentifier("sharing.question")
            .onAppear(perform: arrive)
            .onChange(of: seen, initial: true) { _, seen in
                if seen, !heard {
                    heard = true
                    let words = [approval.title, approval.asker, approval.message].compactMap(\.self)
                    AccessibilityNotification.Announcement(words.joined(separator: " ")).post()
                }
                if !read {
                    readSince = seen ? .now : nil
                }
            }
            .task {
                try? await Task.sleep(for: .milliseconds(500))
                settled = true
            }
            .task(id: readSince) {
                guard let since = readSince else { return }
                let left = since.addingTimeInterval(TimeInterval(approval.readSeconds)).timeIntervalSinceNow
                if left > 0 {
                    try? await Task.sleep(for: .seconds(left))
                }
                guard !Task.isCancelled, readSince == since else { return }
                withAnimation(theme.motion.snappy) { read = true }
            }
            .onReceive(NotificationCenter.default.publisher(for: Self.cameToFront)) { _ in inFront = true }
            .onReceive(NotificationCenter.default.publisher(for: Self.leftFront)) { _ in inFront = false }
    }

    private func arrive() {
        #if os(iOS)
            UINotificationFeedbackGenerator().notificationOccurred(.warning)
        #endif
        withAnimation(theme.motion.snappy?.delay(0.1)) { arrived = true }
    }

    private static let width: CGFloat = 440

    #if os(iOS)
        private static var appInFront: Bool {
            UIApplication.shared.applicationState == .active
        }

        private static let cameToFront = UIApplication.didBecomeActiveNotification
        private static let leftFront = UIApplication.willResignActiveNotification
    #else
        private static var appInFront: Bool {
            NSApp.isActive
        }

        private static let cameToFront = NSApplication.didBecomeActiveNotification
        private static let leftFront = NSApplication.didResignActiveNotification
    #endif
}

private struct CautionMark: View {
    @Environment(\.theme) private var theme
    let arrived: Bool
    private static let size: CGFloat = 44

    var body: some View {
        let shown = arrived || theme.motion.reduced
        Image(systemName: "exclamationmark.shield.fill")
            .font(.system(size: 23, weight: .semibold))
            .foregroundStyle(theme.colors.onDanger)
            .symbolEffect(.wiggle, options: .nonRepeating, value: !theme.motion.reduced && arrived)
            .frame(width: Self.size, height: Self.size)
            .background {
                RoundedRectangle(cornerRadius: Self.size * 0.3, style: .continuous).fill(theme.colors.caution)
            }
            .scaleEffect(shown ? 1 : 0.6)
            .opacity(shown ? 1 : 0)
            .accessibilityHidden(true)
    }
}

/// The tick that comes before the confirm button: the whole row ticks it.
private struct ConsentStyle: ToggleStyle {
    @Environment(\.theme) private var theme

    func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle() } label: {
            HStack(spacing: 10) {
                TickBox(ticked: configuration.isOn)
                configuration.label.appTextStyle(.body).fontWeight(.medium).foregroundStyle(theme.colors.ink)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
            .background { WellBackground(shape: RoundedRectangle(cornerRadius: Radius.md, style: .continuous)) }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityValue(Text(configuration.isOn ? "On" : "Off", bundle: .module))
        .accessibilityAddTraits(configuration.isOn ? [.isToggle, .isSelected] : .isToggle)
    }
}

private struct TickBox: View {
    @Environment(\.theme) private var theme
    let ticked: Bool

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 5, style: .continuous)
        ZStack {
            if ticked {
                BezelBackground(shape: shape, fill: theme.colors.accent)
            } else {
                shape.fill(theme.colors.well.mix(with: .black, by: theme.isDark ? 0.3 : 0.06))
                    .overlay { shape.strokeBorder(theme.colors.inkFaint, lineWidth: 1.5) }
            }
            CheckStroke().trim(from: 0, to: ticked ? 1 : 0)
                .stroke(theme.colors.onAccent, style: StrokeStyle(lineWidth: 2, lineCap: .round, lineJoin: .round))
                .padding(1)
        }
        .frame(width: 20, height: 20)
        .animation(theme.motion.fade, value: ticked)
    }
}

/// The time left before the confirm button works, as a ring that empties and the seconds in it.
private struct ReadRing: View {
    @Environment(\.theme) private var theme
    let since: Date?
    let seconds: Int

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30, paused: since == nil)) { context in
            let total = Double(max(seconds, 1))
            let elapsed = since.map { context.date.timeIntervalSince($0) } ?? 0
            let left = min(total, max(0, total - elapsed))
            ZStack {
                Circle().stroke(theme.colors.hairline, lineWidth: 2)
                Circle().trim(from: 1 - left / total, to: 1)
                    .stroke(theme.colors.inkMuted, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                Text(verbatim: "\(Int(left.rounded(.up)))")
                    .font(.system(size: 10, weight: .semibold).monospacedDigit())
                    .foregroundStyle(theme.colors.inkMuted)
            }
            .frame(width: 18, height: 18)
        }
        .accessibilityHidden(true)
    }
}
