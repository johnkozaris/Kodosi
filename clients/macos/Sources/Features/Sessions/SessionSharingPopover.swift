import SwiftUI

struct SessionSharingPopover: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    let session: RuntimeSession
    @State private var selected: Set<String> = []
    @State private var original: Set<String> = []
    @State private var saving = false
    @State private var error: String?

    private var current: RuntimeSession {
        deps.session(session.id) ?? session
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 10) {
                SessionProgramIcon(program: session.program)
                    .frame(width: 34, height: 34).background(theme.colors.primary.opacity(0.1))
                VStack(alignment: .leading, spacing: 3) {
                    Text("Share terminal").appTextStyle(.headingSection)
                    Text(session.name).appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground).lineLimit(1)
                }
                Spacer()
                SessionIconButton(title: "Close sharing panel", symbol: "xmark", identifier: "sharing.close") { dismiss() }
            }

            let connected = (current.connectedUsers ?? []).filter { $0 != deps.userId }
            if !connected.isEmpty {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Connected now").appTextStyle(.headingItem)
                    ForEach(connected, id: \.self) { id in
                        let friend = deps.friends.first { $0.userId == id }
                        Label(friend?.displayName ?? friend?.handle ?? String(localized: "Connected person"), systemImage: "person.fill")
                            .appTextStyle(.body)
                    }
                }.padding(12).background(theme.colors.surfacePanel)
            }
            if !deps.accountReady {
                Button(deps.signInPrompt ?? String(localized: "Sign in")) {
                    dismiss()
                    deps.beginSignIn()
                }.buttonStyle(SolidPrimaryButtonStyle())
            } else if let roomName = current.missionName {
                Text("Shared with \(roomName)").appTextStyle(.body)
            } else if deps.friends.isEmpty {
                Button("Add a friend") { dismiss(); deps.workbench.section = .people }.buttonStyle(SolidSecondaryButtonStyle())
            } else {
                ScrollView {
                    VStack(spacing: 6) {
                        ForEach(deps.friends) { friend in
                            Button {
                                if selected.contains(friend.userId) {
                                    selected.remove(friend.userId)
                                } else {
                                    selected.insert(friend.userId)
                                }
                            } label: {
                                PopoverActionRow(
                                    icon: selected.contains(friend.userId) ? "checkmark"
                                        : friend.identityChanged ? "exclamationmark.triangle" : "person",
                                    title: (friend.displayName.isEmpty ? friend.handle : friend.displayName)
                                        + (friend.identityChanged ? String(localized: " (trust in People first)") : "")
                                )
                                .background(theme.colors.card.opacity(0.6))
                                .overlay(RoundedRectangle(cornerRadius: theme.radius.sm)
                                    .stroke(selected.contains(friend.userId) ? theme.colors.primary.opacity(0.55) : theme.colors.border, lineWidth: 1))
                            }.buttonStyle(.plain).disabled(saving || (friend.identityChanged && !selected.contains(friend.userId)))
                        }
                    }
                }.frame(maxHeight: 240)
                Button(saving ? "Saving…" : "Save sharing") {
                    saving = true
                    Task { @MainActor in
                        defer { saving = false }
                        do {
                            try await deps.mutateSession("session.share", session: session, fields: [
                                "userIds": .array(selected.sorted().map(JSONValue.string)),
                                "expectedUserIds": .array(original.sorted().map(JSONValue.string)),
                            ])
                            dismiss()
                        } catch { self.error = error.localizedDescription }
                    }
                }.buttonStyle(SolidPrimaryButtonStyle()).disabled(saving || selected == Set(session.sharedWith))
            }
            if let error {
                Text(error).appTextStyle(.caption).foregroundStyle(theme.colors.destructive)
            }
        }.padding(20).frame(width: 360).popoverSurface()
            .onAppear { original = Set(current.sharedWith); selected = original }
            .onChange(of: deps.accountEpoch) { _, _ in dismiss() }
    }
}
