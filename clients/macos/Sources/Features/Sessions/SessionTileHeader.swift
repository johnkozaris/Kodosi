import SwiftUI

struct SessionTileHeader: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let isFocused: Bool
    let roomEmbedded: Bool
    let isSelected: Bool
    let showsActions: Bool
    let viewers: [AvatarStack.Person]
    let isClosing: Bool
    let close: () -> Void

    private var stageId: String {
        AccessibilityIdentifier.stageSession(session.id)
    }

    var body: some View {
        HStack(spacing: 9) {
            if roomEmbedded {
                place
            } else {
                AgentMark(kind: session.agent, size: 20, activity: session.mark(rested: .awake))
                Text(session.name).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
                    .contentTransition(.interpolate).layoutPriority(2)
                if session.activity != nil || session.progress != nil {
                    SessionActivityText(session: session).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        .help(session.activity ?? "")
                }
                if session.kind == .remote {
                    HStack(spacing: 4) {
                        DeviceGlyph(label: session.hostLabel).font(.system(size: 9, weight: .semibold))
                        Text(session.hostLabel).lineLimit(1)
                    }
                    .appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
                    .padding(.horizontal, 7).frame(height: 18).background(theme.colors.ink.opacity(0.07), in: Capsule())
                    .layoutPriority(1)
                }
            }
            Spacer(minLength: 4)
            status
            AvatarStack(people: viewers, size: 20, limit: 3, ring: theme.colors.terminal)
                .help(viewers.isEmpty ? "" : String(localized: "Here now: \(viewers.map(\.name).joined(separator: ", "))"))
            actions
        }
        .padding(.leading, 11).padding(.trailing, 6).frame(height: 38)
    }

    private var place: some View {
        HStack(spacing: 6) {
            DeviceGlyph(label: session.hostLabel).font(.system(size: 11, weight: .medium))
            Text(session.hostLabel).lineLimit(1)
            if let folder = session.folderName {
                Text(verbatim: "·").foregroundStyle(theme.colors.inkFaint)
                Image(systemName: "folder").font(.system(size: 10, weight: .medium))
                Text(folder).lineLimit(1).truncationMode(.middle)
            }
            if session.activity != nil || session.progress != nil {
                Text(verbatim: "·").foregroundStyle(theme.colors.inkFaint)
                SessionActivityText(session: session)
            }
        }
        .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
        .help(session.workingDir ?? session.hostLabel)
    }

    @ViewBuilder
    private var status: some View {
        if session.kind == .remote, session.isConnected, session.status == .reconnecting {
            Text("Reconnecting").appTextStyle(.caption).foregroundStyle(theme.colors.caution).shimmer()
                .accessibilityIdentifier("\(stageId).reconnecting")
        }
        if session.kind == .remote, session.isConnected, let notice = session.message {
            Text(notice).appTextStyle(.caption).foregroundStyle(theme.colors.caution).lineLimit(1)
                .accessibilityIdentifier("\(stageId).notice")
        }
    }

    private var actions: some View {
        HStack(spacing: 0) {
            if session.atPrompt {
                let commands = deps.settings.startCommands.filter { $0.line != nil }
                ForEach(commands) { command in
                    StartMark(command: command, repeated: commands.first { $0.agent == command.agent } != command) {
                        deps.start(command, in: session)
                    }
                    .accessibilityIdentifier("\(stageId).start.\(AccessibilityIdentifier.token(command.id))")
                    .transition(AnyTransition.pop)
                }
            }
            if session.kind == .local, session.isOwner {
                IconButton(title: "Share", symbol: "person.badge.plus", identifier: "\(stageId).share", size: 26,
                           active: deps.workbench.sharingSessionId == session.id)
                {
                    deps.workbench.sharingSessionId = session.id
                }
                .popover(isPresented: Binding(get: { deps.workbench.sharingSessionId == session.id }, set: {
                    if !$0 {
                        deps.workbench.sharingSessionId = nil
                    }
                })) {
                    SessionSharingPopover(session: session).chromeScope()
                }
            }
            IconButton(title: "Details", symbol: "info.circle", identifier: "\(stageId).details", size: 26) {
                deps.workbench.detailsSessionId = session.id
            }
            IconButton(title: "Minimize", symbol: "minus", identifier: "\(stageId).minimize", size: 26) {
                deps.dismissSession(session.id)
            }
            if !roomEmbedded {
                IconButton(title: isFocused ? "Show all" : "Zoom",
                           symbol: isFocused ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right",
                           identifier: "\(stageId).focus", size: 26)
                {
                    deps.workbench.toggleFocus(session.id)
                }
            }
            IconButton(title: "Close terminal", symbol: "xmark", identifier: "\(stageId).close", size: 26, destructive: true, action: close)
                .disabled(!session.canControl || isClosing)
        }
        .opacity(showsActions ? 1 : 0.28)
        .animation(theme.motion.snappy, value: session.atPrompt)
    }
}

struct StartMark: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let command: StartCommand
    let repeated: Bool
    var size: CGFloat = 16
    let action: () -> Void

    private var initial: String {
        command.name.split(separator: " ").last?.first.map { String($0).uppercased() } ?? ""
    }

    var body: some View {
        Button(action: action) {
            AgentMark(kind: command.agent, size: size)
                .overlay {
                    if repeated {
                        RoundedRectangle(cornerRadius: size * 0.3, style: .continuous).fill(command.agent.tint)
                        Text(initial).font(.system(size: size * 0.6, weight: .bold, design: .rounded)).foregroundStyle(.white)
                    }
                }
                .frame(width: size + 10, height: size + 10)
                .background(theme.colors.ink.opacity(hovered ? 0.08 : 0), in: RoundedRectangle(cornerRadius: Radius.xs, style: .continuous))
                .contentShape(Rectangle())
        }
        .buttonStyle(PressScaleStyle(scale: 0.9))
        .onHover { hovered = $0 }
        .help(String(localized: "Start \(command.name)"))
        .accessibilityLabel(Text("Start \(command.name)"))
    }
}
