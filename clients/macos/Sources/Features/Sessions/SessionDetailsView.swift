import SwiftUI

struct SessionDetailsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    @State private var sharing = false
    @State private var name = ""
    @State private var confirmingLeave = false
    @State private var error: String?

    private var current: RuntimeSession? {
        deps.session(session.id)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            HStack(spacing: 14) {
                AgentMark(kind: (current ?? session).agent, size: 44, activity: (current ?? session).isWorking ? .working : .awake)
                VStack(alignment: .leading, spacing: 2) {
                    if current?.isOwner == true {
                        TextField("Name", text: $name)
                            .textFieldStyle(.plain).appTextStyle(.title).foregroundStyle(theme.colors.ink)
                            .onSubmit {
                                if let current {
                                    deps.rename(current, to: name)
                                }
                            }
                            .accessibilityIdentifier("panel.sessionDetails.name")
                    } else {
                        Text(current?.name ?? session.name).appTextStyle(.title).foregroundStyle(theme.colors.ink)
                    }
                    Text((current ?? session).agent.label).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                }
                Spacer()
                IconButton(title: "Close", symbol: "xmark", identifier: "panel.sessionDetails.close") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            if let current {
                ListGroup {
                    ListRow(current.hostLabel, subtitle: current.isConnected ? nil : current.message ?? current.statusLabel,
                            symbol: DeviceGlyph.symbol(for: current.hostLabel), tint: TileTint.graphite)
                    if let path = current.workingDir {
                        ListRow(URL(fileURLWithPath: path).lastPathComponent, subtitle: path, monoSubtitle: true,
                                symbol: "folder.fill", tint: TileTint.amber)
                        {
                            if let directory = current.localDirectory {
                                Button("Open") { do { try NativeFiles.open(directory) } catch { self.error = error.localizedDescription } }
                                    .buttonStyle(.kodosi(.secondary, size: .small))
                            }
                        }
                    }
                    access(current)
                }
                if !current.isOwner, current.missionId == nil {
                    Button("Leave this terminal…") { confirmingLeave = true }
                        .buttonStyle(.kodosi(.ghost)).foregroundStyle(theme.colors.danger)
                        .accessibilityIdentifier("panel.sessionDetails.leave")
                }
            } else {
                Text("This terminal has ended.").appTextStyle(.body).foregroundStyle(theme.colors.inkMuted)
            }
            if let error {
                ErrorNote(message: error)
            }
        }
        .padding(24)
        .frame(width: 480).background(theme.colors.raised.mix(with: theme.colors.surface, by: 0.4))
        .buttonStyle(.kodosi(.secondary))
        .onAppear { name = current?.name ?? session.name }
        .onChange(of: current?.name) { _, value in
            if let value {
                name = value
            }
        }
        .onChange(of: deps.accountEpoch) { _, _ in dismiss() }
        .confirmationDialog("Leave this terminal?", isPresented: $confirmingLeave, titleVisibility: .visible) {
            Button("Leave", role: .destructive) { leave() }
        } message: {
            Text("You lose access until the owner shares it again. The terminal keeps running.")
        }
    }

    @ViewBuilder
    private func access(_ current: RuntimeSession) -> some View {
        if deps.userId == nil {
            ListRow(String(localized: "Only on this Mac"), subtitle: String(localized: "Sign in to share it"), symbol: "lock.fill", tint: TileTint.graphite)
        } else if !deps.localDeviceEnrolled {
            ListRow(String(localized: "Only on this Mac"), subtitle: String(localized: "Trust this Mac to share it"),
                    symbol: "lock.fill", tint: TileTint.graphite)
        } else if !current.isOwner {
            ListRow(String(localized: "Shared by \(current.ownerName ?? current.hostLabel)"),
                    subtitle: current.missionName.map { String(localized: "In \($0)") }, symbol: "person.2.fill", tint: TileTint.blue)
        } else {
            let names = current.sharedWith.map(deps.personName)
            let title = current.missionName.map { String(localized: "Shared with \($0)") }
                ?? (names.isEmpty ? String(localized: "Only you and your devices") : names.joined(separator: ", "))
            ListRow(title, subtitle: String(localized: "Encrypted end to end"), symbol: "person.2.fill", tint: TileTint.blue) {
                if current.kind == .local {
                    Button("Share…") { sharing = true }
                        .buttonStyle(.kodosi(.tinted, size: .small))
                        .popover(isPresented: $sharing) { SessionSharingPopover(session: current) }
                }
            }
        }
    }

    private func leave() {
        guard let current else { return }
        error = nil
        Task { @MainActor in
            do {
                try await deps.mutateSession("session.leave", session: current)
                dismiss()
            } catch { self.error = error.localizedDescription }
        }
    }
}
