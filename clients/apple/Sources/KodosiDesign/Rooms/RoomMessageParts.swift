import KodosiBridge
import SwiftUI

/// The day of the messages that follow it.
public struct RoomDayDivider: View {
    #if os(iOS)
        private static let height: CGFloat = 22
        private static let top: CGFloat = 16
    #else
        private static let height: CGFloat = 20
        private static let top: CGFloat = 14
    #endif

    @Environment(\.theme) private var theme
    let date: Date

    public init(date: Date) {
        self.date = date
    }

    public var body: some View {
        Text(RoomDates.day(date))
            .appTextStyle(.caption)
            .foregroundStyle(theme.colors.inkFaint)
            .padding(.horizontal, 10)
            .frame(minHeight: Self.height)
            .background(theme.colors.ink.opacity(0.06), in: Capsule())
            .frame(maxWidth: .infinity)
            .padding(.top, Self.top)
            .padding(.bottom, 4)
            .accessibilityAddTraits(.isHeader)
    }
}

/// The face of who wrote a message: the mark of the agent, or the person.
public struct RoomMessageFace: View {
    let message: RoomMessage
    let terminal: RuntimeSession?

    public init(message: RoomMessage, terminal: RuntimeSession?) {
        self.message = message
        self.terminal = terminal
    }

    public var body: some View {
        if message.agent != nil {
            AgentMark(
                kind: AgentKind(agentName: message.agent, program: terminal?.program), size: 28,
                activity: terminal?.mark(rested: .awake) ?? .awake,
            )
        } else {
            PersonAvatar(name: message.authorName, key: message.authorId, size: 28)
        }
    }
}

/// Who wrote a message: the person with their user name, or the agent and whom it works for.
public struct RoomMessageAuthor: View {
    @Environment(\.theme) private var theme
    let message: RoomMessage
    let handle: String?

    public init(message: RoomMessage, handle: String?) {
        self.message = message
        self.handle = handle
    }

    public var body: some View {
        Text(message.agent ?? message.authorName)
            .appTextStyle(.subhead)
            .foregroundStyle(theme.colors.ink)
            .lineLimit(1)
        if message.agent != nil {
            Text("for \(message.authorName)", bundle: .module)
                .appTextStyle(.caption)
                .fontWeight(.regular)
                .foregroundStyle(theme.colors.inkFaint)
                .lineLimit(1)
        } else if let handle {
            Text(verbatim: "@\(handle)")
                .appTextStyle(.caption)
                .fontWeight(.regular)
                .foregroundStyle(theme.colors.inkFaint)
                .lineLimit(1)
        }
    }
}

/// When a message was written.
public struct RoomMessageTime: View {
    @Environment(\.theme) private var theme
    let date: Date

    public init(date: Date) {
        self.date = date
    }

    public var body: some View {
        Text(date, format: .dateTime.hour().minute())
            .appTextStyle(.caption2)
            .foregroundStyle(theme.colors.inkFaint)
    }
}

/// The words of a message, with its mentions and links.
public struct RoomMessageBody: View {
    #if os(iOS)
        private static let lineSpacing: CGFloat = 2
    #else
        private static let lineSpacing: CGFloat = 2.5
    #endif

    @Environment(\.theme) private var theme
    let text: String
    let targets: [RoomMention]

    public init(text: String, targets: [RoomMention]) {
        self.text = text
        self.targets = targets
    }

    public var body: some View {
        RoomMessageText(text: text, targets: targets)
            .appTextStyle(.body)
            .lineSpacing(Self.lineSpacing)
            .foregroundStyle(theme.colors.ink)
            .tint(theme.colors.accentStrong)
        #if os(macOS)
            .textSelection(.enabled)
        #endif
    }
}

/// The shape of your own message: round, but for the corner that points at you.
public struct RoomBubble: Shape {
    public init() {}

    public func path(in rect: CGRect) -> Path {
        UnevenRoundedRectangle(topLeadingRadius: 16, bottomLeadingRadius: 16, bottomTrailingRadius: 6, topTrailingRadius: 16, style: .continuous)
            .path(in: rect)
    }
}

/// The terminal that an agent wrote from, as a chip.
public struct RoomTerminalChip: View {
    #if os(iOS)
        private static let spacing: CGFloat = 5
        private static let inset: CGFloat = 8
        private static let height: CGFloat = 22
    #else
        private static let spacing: CGFloat = 4
        private static let inset: CGFloat = 7
        private static let height: CGFloat = 18
    #endif

    @Environment(\.theme) private var theme
    #if os(iOS)
        @ScaledMetric(relativeTo: .caption) private var glyph: CGFloat = 9
    #else
        private let glyph: CGFloat = 8
    #endif
    let name: String
    let sign: StatusSign.Form?

    public init(name: String, sign: StatusSign.Form? = nil) {
        self.name = name
        self.sign = sign
    }

    public var body: some View {
        HStack(spacing: Self.spacing) {
            Image(systemName: "apple.terminal").font(.system(size: glyph, weight: .bold))
            Text(name).lineLimit(1)
            if let sign {
                StatusSign(form: sign, size: 9)
            }
        }
        .appTextStyle(.caption)
        .foregroundStyle(theme.colors.inkMuted)
        .padding(.horizontal, Self.inset)
        .frame(minHeight: Self.height)
        .background(theme.colors.ink.opacity(0.08), in: Capsule())
    }
}

public extension View {
    /// Your own message, in a copper-soft bubble.
    func roomBubble() -> some View {
        modifier(RoomBubbleFill())
    }

    /// A message of someone else that mentions you, tinted copper.
    func roomMention(_ mentioned: Bool) -> some View {
        modifier(RoomMentionTint(mentioned: mentioned))
    }
}

private struct RoomBubbleFill: ViewModifier {
    @Environment(\.theme) private var theme

    func body(content: Content) -> some View {
        content
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(RoomBubble().fill(theme.colors.accentSoft))
    }
}

private struct RoomMentionTint: ViewModifier {
    @Environment(\.theme) private var theme
    let mentioned: Bool

    func body(content: Content) -> some View {
        content
            .padding(.horizontal, mentioned ? 8 : 0)
            .padding(.vertical, mentioned ? 6 : 0)
            .background {
                if mentioned {
                    RoundedRectangle(cornerRadius: Radius.md, style: .continuous).fill(theme.colors.accentSoft.opacity(0.6))
                }
            }
    }
}
