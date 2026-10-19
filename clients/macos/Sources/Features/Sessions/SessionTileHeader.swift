import KodosiBridge
import KodosiDesign
import SwiftUI

struct SessionTileHeader: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let isFocused: Bool
    let roomEmbedded: Bool
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
                AgentMark(session: session, size: 20, activity: session.mark(rested: .awake))
                Text(session.name).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
                    .contentTransition(.interpolate).layoutPriority(2)
                if session.activity != nil || session.progress != nil {
                    SessionActivityText(session: session).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        .help(session.activity ?? "")
                }
                if let sign = deps.sign(of: session) {
                    StatusSign(form: sign).transition(AnyTransition.pop)
                }
                if session.kind == .remote {
                    tag {
                        HStack(spacing: 4) {
                            DeviceGlyph(label: session.hostLabel).font(.system(size: 9, weight: .semibold))
                            Text(session.hostLabel).lineLimit(1)
                        }
                    }
                }
                if let place = session.place {
                    tag { SessionPlace(place: place, glyphFont: .system(size: 9, weight: .semibold)) }
                        .help(session.placeLabel ?? "")
                }
            }
            Spacer(minLength: 4)
            if let (item, question) = deps.dosi.question(forTerminal: session) {
                DosiKeys(item: item, question: question).transition(AnyTransition.pop)
            }
            status
            AvatarStack(people: viewers, size: 20, limit: 3, ring: theme.colors.terminal)
                .help(viewers.isEmpty ? "" : String(localized: "Here now: \(viewers.map(\.name).joined(separator: ", "))"))
            actions
        }
        .padding(.leading, 11).padding(.trailing, 6).frame(height: 38)
    }

    private func tag(@ViewBuilder _ content: () -> some View) -> some View {
        content()
            .appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
            .padding(.horizontal, 7).frame(height: 18).background(theme.colors.ink.opacity(0.07), in: Capsule())
            .layoutPriority(1)
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
                ForEach(deps.launcher.installed) { info in
                    StartMark(kind: info.kind) { deps.start(info.agent, in: session) }
                        .accessibilityIdentifier("\(stageId).start.\(info.agent)")
                        .transition(AnyTransition.pop)
                }
            }
            if session.kind == .local, session.isOwner {
                IconButton(title: "Share", symbol: "person.badge.plus", identifier: "\(stageId).share", size: 26,
                           active: deps.workbench.sharingSessionId == session.id)
                {
                    deps.workbench.sharingSessionId = session.id
                }
                .kodosiPopover(isPresented: Binding(get: { deps.workbench.sharingSessionId == session.id }, set: {
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

struct DosiKeys: View {
    @Environment(AppDependencies.self) private var deps
    let item: DosiItem
    let question: DosiQuestion

    var body: some View {
        HStack(spacing: 4) {
            DosiFace(mood: .asking, size: 16).help(question.question)
            if question.multiple {
                Button("Answer") { deps.workbench.openDosi() }.buttonStyle(.kodosi(.tinted, size: .small))
            } else {
                ForEach(question.options.prefix(4), id: \.self) { option in
                    Button(option) { deps.answerDosi(item, [option]) }
                        .buttonStyle(.kodosi(.secondary, size: .small))
                        .accessibilityIdentifier("dosi.keys.\(AccessibilityIdentifier.token(option))")
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text(question.question))
    }
}

struct StartMark: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let kind: AgentKind
    var size: CGFloat = 16
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            AgentMark(kind: kind, size: size)
                .frame(width: size + 10, height: size + 10)
                .background(theme.colors.ink.opacity(hovered ? 0.08 : 0), in: RoundedRectangle(cornerRadius: Radius.xs, style: .continuous))
                .contentShape(Rectangle())
        }
        .buttonStyle(PressScaleStyle(scale: 0.9))
        .onHover { hovered = $0 }
        .help(String(localized: "Start \(kind.label)"))
        .accessibilityLabel(Text("Start \(kind.label)"))
    }
}
