import SwiftUI

struct SessionDetailsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    @State private var sharing = false
    @State private var missionId = ""
    @State private var saving = false
    @State private var confirmingLeave = false
    @State private var error: String?

    private var current: RuntimeSession? {
        deps.session(session.id)
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                SessionProgramIcon(program: current?.program)
                Text(current?.name ?? session.name).appTextStyle(.headingSection)
                Spacer()
                SessionIconButton(title: "Close details", symbol: "xmark", identifier: "panel.sessionDetails.close") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }.padding(20).background(theme.colors.surfacePanel).seamBorder(.bottom)
            ScrollView {
                VStack(alignment: .leading, spacing: 24) {
                    if let current {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("Folder").appTextStyle(.headingItem).foregroundStyle(theme.colors.primary)
                            if let path = current.workingDir {
                                Text(URL(fileURLWithPath: path).lastPathComponent).appTextStyle(.headingSection)
                                Text(path).appTextStyle(.monoCaption).foregroundStyle(theme.colors.mutedForeground).textSelection(.enabled)
                            }
                            Text(current.hostLabel).appTextStyle(.body)
                            if let directory = current.localDirectory {
                                Button("Open folder") { do { try NativeFiles.open(directory) } catch { self.error = error.localizedDescription } }
                            }
                            if !current.isConnected {
                                Text(current.message ?? current.statusLabel).appTextStyle(.body).foregroundStyle(theme.colors.statusWaiting)
                            }
                        }
                        Divider()
                        VStack(alignment: .leading, spacing: 12) {
                            HStack {
                                Text("People").appTextStyle(.headingItem).foregroundStyle(theme.colors.primary)
                                Spacer()
                                if current.kind == .local, current.isOwner {
                                    Button("Manage sharing") { sharing = true }
                                        .popover(isPresented: $sharing) { SessionSharingPopover(session: current) }
                                }
                            }
                            if deps.userId == nil {
                                Text("Only available on this Mac while signed out.").appTextStyle(.body)
                            } else if !deps.localDeviceEnrolled {
                                Text("Only available on this Mac until this Mac is trusted.").appTextStyle(.body)
                            } else if !current.isOwner {
                                Text("Shared by \(current.ownerName ?? current.hostLabel)").appTextStyle(.body)
                                if current.missionId == nil {
                                    Button("Leave shared terminal…") { confirmingLeave = true }
                                        .accessibilityIdentifier("panel.sessionDetails.leave")
                                }
                            } else {
                                Text("Your approved devices").appTextStyle(.body)
                                ForEach(current.sharedWith, id: \.self) { id in
                                    let friend = deps.friends.first { $0.userId == id }
                                    HStack {
                                        Text(friend?.displayName ?? friend?.handle ?? String(localized: "Shared friend")).appTextStyle(.body)
                                        Spacer()
                                        if (current.connectedUsers ?? []).contains(id) {
                                            Text("Connected now").appTextStyle(.caption).foregroundStyle(theme.colors.primary)
                                        }
                                    }
                                }
                            }
                        }
                        if current.isOwner, deps.accountReady {
                            Divider()
                            VStack(alignment: .leading, spacing: 10) {
                                Text("Room").appTextStyle(.headingItem).foregroundStyle(theme.colors.primary)
                                HStack {
                                    KodosiPicker("Share with room", selection: $missionId, values: [""] + deps.missions.map(\.id), label: { id in
                                        deps.missions.first { $0.id == id }?.name ?? String(localized: "None")
                                    })
                                    Button(saving ? "Saving…" : "Save") { saveMission(current) }
                                        .disabled(saving || missionId == (current.missionId ?? ""))
                                }
                            }
                        } else if let mission = current.missionName {
                            Divider()
                            LabeledContent("Room", value: mission).appTextStyle(.body)
                        }
                    } else {
                        Text("This terminal has ended.").appTextStyle(.body)
                    }
                    if let error {
                        Text(error).appTextStyle(.body).foregroundStyle(theme.colors.destructive)
                    }
                }.padding(24).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .frame(width: 580, height: 520).background(theme.colors.background)
        .buttonStyle(SolidSecondaryButtonStyle())
        .onAppear { missionId = current?.missionId ?? "" }
        .onChange(of: deps.accountEpoch) { _, _ in dismiss() }
        .confirmationDialog("Leave this shared terminal?", isPresented: $confirmingLeave, titleVisibility: .visible) {
            Button("Leave", role: .destructive) { leave() }
        } message: {
            Text("You lose access until the owner shares it with you again. The terminal keeps running.")
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

    private func saveMission(_ current: RuntimeSession) {
        saving = true
        error = nil
        Task { @MainActor in
            defer { saving = false }
            do {
                try await deps.mutateSession(
                    "session.attachMission", session: current,
                    fields: ["missionId": missionId.isEmpty ? .null : .string(missionId)]
                )
            } catch { self.error = error.localizedDescription }
        }
    }
}
