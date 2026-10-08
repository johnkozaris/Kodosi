import SwiftUI

struct SessionSidebarView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var collapsedFolders: Set<String> = []

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                if !deps.workbench.sidebarCollapsed {
                    Text("Terminals").appTextStyle(.headingItem).foregroundStyle(theme.colors.mutedForeground)
                    Spacer()
                }
                SessionIconButton(title: deps.workbench.sidebarCollapsed ? "Expand sidebar" : "Collapse sidebar",
                                  symbol: "sidebar.left", identifier: "sidebar.toggle")
                {
                    deps.workbench.sidebarCollapsed.toggle()
                }
            }.padding(.horizontal, 8).padding(.top, 6)
            if deps.workbench.sidebarCollapsed {
                SessionIconButton(title: "New Session", symbol: "plus", identifier: "sidebar.newSession") { deps.newSession() }
                    .padding(.top, 8)
                Spacer()
                SessionIconButton(title: "History", symbol: "clock.arrow.circlepath", identifier: "sidebar.resume") {
                    deps.workbench.showsHistory = true
                }.padding(.bottom, 12)
            } else {
                HStack(spacing: 0) {
                    Button { deps.newSession() } label: {
                        Label("New session", systemImage: "plus").frame(maxWidth: .infinity, minHeight: 34)
                    }.accessibilityIdentifier("sidebar.newSession")
                    Rectangle().fill(theme.colors.primaryForeground.opacity(0.2)).frame(width: 1, height: 20)
                    Button {
                        if let directory = pickWorkingDirectory(initialDirectory: deps.selectedLocalDirectory) {
                            deps.newSession(directory: directory)
                        }
                    } label: {
                        Image(systemName: "folder.badge.plus").frame(width: 34, height: 34).contentShape(Rectangle())
                    }.help("New session in folder").accessibilityLabel(Text("New session in folder"))
                        .accessibilityIdentifier("sidebar.newInFolder")
                }.buttonStyle(.plain).appTextStyle(.button).foregroundStyle(theme.colors.primaryForeground)
                    .background(ElevatedSurface(fill: theme.colors.primary, isPressed: false)).padding(12)
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 2) {
                        ForEach(SessionFolderGroup.groups(deps.sessions)) { group in
                            VStack(spacing: 2) {
                                HStack(spacing: 4) {
                                    Button {
                                        if collapsedFolders.contains(group.id) {
                                            collapsedFolders.remove(group.id)
                                        } else {
                                            collapsedFolders.insert(group.id)
                                        }
                                    } label: {
                                        HStack(spacing: 6) {
                                            Image(systemName: collapsedFolders.contains(group.id) ? "chevron.right" : "chevron.down").font(.system(size: 9))
                                            Image(systemName: "folder").foregroundStyle(theme.colors.primary)
                                            VStack(alignment: .leading, spacing: 2) {
                                                Text(group.name).appTextStyle(.headingItem).lineLimit(1)
                                                if let host = group.host {
                                                    Text(host).appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground).lineLimit(1)
                                                }
                                            }
                                            Spacer(minLength: 0)
                                        }.frame(minHeight: 30).contentShape(Rectangle())
                                    }.buttonStyle(.plain).help(group.directory ?? group.name)
                                        .accessibilityValue(Text(collapsedFolders.contains(group.id) ? "Collapsed" : "Expanded"))
                                    if group.host == nil, let directory = group.directory {
                                        SessionIconButton(
                                            title: "New session in folder", symbol: "plus",
                                            identifier: "sidebar.folder.\(AccessibilityIdentifier.token(group.id)).new"
                                        ) {
                                            deps.newSession(directory: directory)
                                        }
                                    }
                                }.padding(.horizontal, 8)
                                if !collapsedFolders.contains(group.id) {
                                    ForEach(group.sessions) { session in SessionSidebarRow(session: session).padding(.leading, 12) }
                                }
                            }.padding(.bottom, 6)
                        }
                    }
                }
                Spacer(minLength: 0)
                Button { deps.workbench.showsHistory = true } label: {
                    Label("History", systemImage: "clock.arrow.circlepath").lineLimit(1).fixedSize(horizontal: true, vertical: false)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .buttonStyle(SolidSecondaryButtonStyle()).padding(12).accessibilityIdentifier("sidebar.resume")
            }
        }
        .background(theme.colors.surfacePanel).seamBorder(.trailing)
    }
}

private struct SessionSidebarRow: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    @State private var hovered = false
    @State private var confirmingClose = false

    private var staged: Bool {
        deps.workbench.stagedSessionIds.contains(session.id)
    }

    private var title: String {
        session.name
    }

    var body: some View {
        HStack(spacing: 6) {
            Button { deps.activateSession(session.id) } label: {
                HStack(spacing: 9) {
                    SessionProgramIcon(program: session.program).opacity(staged ? 1 : 0.55)
                        .overlay(alignment: .bottomTrailing) {
                            if session.connectionState == .blocked
                                || (session.connectionState == .offline && (session.status == .reconnecting || session.message != nil))
                            {
                                Circle().fill(theme.colors.statusWaiting).frame(width: 5, height: 5)
                            }
                        }
                    VStack(alignment: .leading, spacing: 3) {
                        Text(title).lineLimit(1).appTextStyle(.body).fontWeight(staged ? .medium : .regular)
                    }
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier(AccessibilityIdentifier.sidebarSession(session.id))
            HStack(spacing: 2) {
                if staged {
                    SessionIconButton(title: "Minimize", symbol: "minus", identifier: "\(AccessibilityIdentifier.sidebarSession(session.id)).minimize") {
                        deps.dismissSession(session.id)
                    }
                }
                SessionIconButton(
                    title: "Close terminal", symbol: "xmark",
                    identifier: "\(AccessibilityIdentifier.sidebarSession(session.id)).close", destructive: true
                ) {
                    confirmingClose = true
                }.disabled(!session.canControl)
            }
            .opacity(hovered ? 1 : 0)
            .allowsHitTesting(hovered)
        }
        .buttonStyle(.plain)
        .foregroundStyle(deps.workbench.selectedSessionId == session.id ? theme.colors.primary : theme.colors.foreground)
        .padding(.horizontal, 8).padding(.vertical, 3)
        .background(deps.workbench.selectedSessionId == session.id ? theme.colors.secondary : hovered ? theme.colors.surfaceStage : .clear)
        .onHover { hovered = $0 }
        .help(session.isConnected ? (staged ? String(localized: "On stage") : String(localized: "Minimized")) : session.statusLabel)
        .contextMenu {
            Button("Open Terminal") { deps.activateSession(session.id) }
            if session.isOwner, session.kind == .local {
                Button("Share…") { deps.activateSession(session.id); deps.workbench.sharingSessionId = session.id }
            }
            Button("Details") { deps.workbench.detailsSessionId = session.id }
            if staged {
                Button("Minimize") { deps.dismissSession(session.id) }
            }
            Button("Close Terminal", role: .destructive) { confirmingClose = true }.disabled(!session.canControl)
        }
        .confirmationDialog("Close \(title)?", isPresented: $confirmingClose, titleVisibility: .visible) {
            Button("Close", role: .destructive) {
                Task { @MainActor in
                    do {
                        try await deps.mutateSession("session.close", session: session)
                    } catch { deps.errorMessage = error.localizedDescription }
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: { Text("Running programs will stop.") }
    }
}

struct SessionProgramIcon: View {
    @Environment(\.theme) private var theme
    let program: String?

    var body: some View {
        Group {
            switch program {
            case "claude": Image("ProviderClaude").renderingMode(.template).resizable().scaledToFit()
            case "copilot": Image("ProviderCopilot").renderingMode(.template).resizable().scaledToFit()
            case "codex": Image("ProviderCodex").renderingMode(.template).resizable().scaledToFit()
            case "cursor": Image("ProviderCursor").renderingMode(.template).resizable().scaledToFit()
            default: Image(systemName: "terminal")
            }
        }
        .foregroundStyle(program == "claude" ? Color(red: 0.851, green: 0.467, blue: 0.341) : theme.colors.foreground)
        .frame(width: 20, height: 20).accessibilityHidden(true)
    }
}
