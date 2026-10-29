import KodosiBridge
import KodosiDesign
import SwiftUI

struct SessionTileHeader: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.canvasPlace) private var canvasPlace
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

    private var canvasOpen: Bool {
        canvasPlace.open == session.id
    }

    var body: some View {
        HStack(spacing: 9) {
            if roomEmbedded {
                Location(host: session.hostLabel, glyph: "folder", name: session.folderName)
                    .help(session.workingDir ?? session.hostLabel)
                if session.activity != nil || session.progress != nil {
                    SessionActivityText(session: session).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                }
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
                if let place = session.place {
                    Location(host: session.kind == .remote ? session.hostLabel : nil, glyph: PlaceGlyph.symbol(for: place.kind), name: place.name)
                        .help(session.placeLabel ?? session.hostLabel)
                        .layoutPriority(1)
                } else if session.kind == .remote {
                    Location(host: session.hostLabel, glyph: "folder", name: nil).layoutPriority(1)
                }
            }
            Spacer(minLength: 4)
            if deps.workbench.dosiOn, let (item, question) = deps.dosi.question(forTerminal: session) {
                DosiKeys(item: item, question: question).transition(AnyTransition.pop)
            }
            status
            AvatarStack(people: viewers, size: 20, limit: 3, ring: theme.colors.terminal)
                .help(viewers.isEmpty ? "" : String(localized: "Here now: \(viewers.map(\.name).joined(separator: ", "))"))
            if session.canvas != nil, !canvasOpen {
                CanvasDock(session: session, store: deps.canvas, chosen: nil, flight: canvasPlace.flight, identifier: "\(stageId).canvas") { section in
                    canvasPlace.show(session, section)
                }
                .transition(AnyTransition.pop)
            }
            actions
        }
        .padding(.leading, 11).padding(.trailing, canvasOpen ? 6 + CanvasBody.clearance : 6).frame(height: 38)
    }

    @ViewBuilder
    private var status: some View {
        if session.kind == .remote, session.isConnected, session.status == .reconnecting {
            Text("Reconnecting").appTextStyle(.caption).foregroundStyle(theme.colors.caution).shimmer()
                .accessibilityIdentifier("\(stageId).reconnecting")
        }
        if session.kind == .remote, session.isConnected, session.status == .offline {
            Text(session.statusLabel).appTextStyle(.caption).foregroundStyle(theme.colors.caution).lineLimit(1)
                .accessibilityIdentifier("\(stageId).offline")
        }
        if session.kind == .remote, session.isConnected, let notice = session.message {
            Text(notice).appTextStyle(.caption).foregroundStyle(theme.colors.caution).lineLimit(1)
                .accessibilityIdentifier("\(stageId).notice")
        }
    }

    private var actions: some View {
        HStack(spacing: 0) {
            TerminalActions(session: session, roomEmbedded: roomEmbedded, stageId: stageId)
            if !canvasOpen {
                IconButton(title: "Minimize", symbol: "minus", identifier: "\(stageId).minimize", size: 26) {
                    deps.dismissSession(session.id)
                }
            }
            if !roomEmbedded, !canvasOpen {
                IconButton(title: isFocused ? "Show all" : "Zoom",
                           symbol: isFocused ? "arrow.down.right.and.arrow.up.left" : "arrow.up.left.and.arrow.down.right",
                           identifier: "\(stageId).focus", size: 26)
                {
                    deps.workbench.toggleFocus(session.id)
                }
            }
            IconButton(title: "Close terminal", symbol: "xmark", identifier: "\(stageId).close", size: 26, destructive: true, action: close)
                .disabled(!session.canClose || isClosing)
        }
        .opacity(showsActions ? 1 : 0.28)
    }
}

/// Where a terminal runs: its computer, then the folder or place in it.
private struct Location: View {
    @Environment(\.theme) private var theme
    let host: String?
    let glyph: String
    let name: String?

    var body: some View {
        HStack(spacing: 4) {
            if let host {
                DeviceGlyph(label: host).font(.system(size: 9.5, weight: .semibold))
                Text(host).lineLimit(1)
            }
            if host != nil, name != nil {
                Image(systemName: "chevron.compact.right").font(.system(size: 11, weight: .medium))
                    .foregroundStyle(theme.colors.inkFaint).padding(.horizontal, 1)
            }
            if let name {
                HStack(spacing: 4) {
                    Image(systemName: glyph).font(.system(size: 9.5, weight: .semibold))
                    Text(name).lineLimit(1).truncationMode(.middle)
                }
                .foregroundStyle(theme.colors.ink.mix(with: theme.colors.inkMuted, by: 0.35))
            }
        }
        .appTextStyle(.caption).foregroundStyle(theme.colors.inkMuted)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(label))
    }

    private var label: String {
        switch (host, name) {
        case let (host?, name?): String(localized: "\(name) on \(host)")
        case let (host?, nil): host
        case let (nil, name?): name
        case (nil, nil): ""
        }
    }
}

/// One accent key for what a person does with a terminal: start an agent, share it, open its
/// browser, see its details, or take it out of the room.
private struct TerminalActions: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.canvasPlace) private var canvasPlace
    let session: RuntimeSession
    let roomEmbedded: Bool
    let stageId: String
    @State private var open = false
    @State private var hovered = false

    var body: some View {
        Button { open.toggle() } label: {
            Image(systemName: "slider.horizontal.3")
                .font(.system(size: 11.5, weight: .semibold))
                .foregroundStyle(theme.colors.accentStrong)
                .frame(width: 26, height: 26)
                .background {
                    RoundedRectangle(cornerRadius: Radius.xs, style: .continuous)
                        .fill(open || hovered ? theme.colors.accentSoft : .clear)
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(PressScaleStyle(scale: 0.9))
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .help("Actions")
        .accessibilityLabel(Text("Actions"))
        .accessibilityIdentifier("\(stageId).actions")
        .kodosiPopover(isPresented: $open, arrowEdge: .bottom, alignment: .trailing) { menu.chromeScope() }
        .background {
            Color.clear.kodosiPopover(isPresented: Binding(get: { deps.workbench.sharingSessionId == session.id }, set: {
                if !$0 {
                    deps.workbench.sharingSessionId = nil
                }
            })) {
                SessionSharingPopover(session: session).chromeScope()
            }
        }
    }

    private var menu: some View {
        VStack(alignment: .leading, spacing: 2) {
            if session.atPrompt, !deps.launcher.installed.isEmpty {
                PanelMenuHeader(title: "Start an agent")
                ForEach(deps.launcher.installed) { info in
                    PanelMenuRow(Text(info.kind.label), icon: .agent(info.kind), identifier: "\(stageId).start.\(info.agent)") {
                        deps.start(info.agent, in: session)
                    }
                }
                PanelMenuDivider()
            }
            if session.kind == .local, session.isOwner {
                PanelMenuRow(Text("Share…"), icon: .symbol("person.badge.plus"), identifier: "\(stageId).share") {
                    deps.workbench.sharingSessionId = session.id
                }
            }
            if session.canControl {
                PanelMenuRow(Text("Open Browser"), icon: .symbol("globe"), identifier: "\(stageId).browser") {
                    deps.openBrowser(of: session)
                    canvasPlace.show(session, nil)
                }
            }
            PanelMenuRow(Text("Details"), icon: .symbol("info.circle"), identifier: "\(stageId).details") {
                deps.workbench.detailsSessionId = session.id
            }
            if roomEmbedded, session.isOwner {
                PanelMenuDivider()
                PanelMenuRow(Text("Remove from room"), icon: .symbol("rectangle.portrait.and.arrow.right"),
                             identifier: "\(stageId).removeFromRoom", destructive: true)
                {
                    Task { @MainActor in
                        do {
                            try await deps.removeFromRoom(session)
                        } catch { deps.errorMessage = error.localizedDescription }
                    }
                }
            }
        }
        .padding(6).frame(width: 220)
    }
}

struct DosiKeys: View {
    @Environment(AppDependencies.self) private var deps
    let item: DosiItem
    let question: DosiQuestion

    private var brief: Bool {
        !question.multiple && question.options.count <= 3 && question.options.allSatisfy { $0.count <= 12 }
    }

    var body: some View {
        HStack(spacing: 4) {
            DosiFace(mood: .asking, size: 16).help(question.question)
            if brief {
                ForEach(question.options, id: \.self) { option in
                    Button(option) { deps.answerDosi(item, [option]) }
                        .buttonStyle(.kodosi(.secondary, size: .small))
                        .accessibilityIdentifier("dosi.keys.\(AccessibilityIdentifier.token(option))")
                }
            } else {
                Button("Answer") { deps.workbench.openDosi() }.buttonStyle(.kodosi(.tinted, size: .small))
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel(Text(question.question))
    }
}
