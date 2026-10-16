import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit

struct FriendDetailView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    let userId: String

    private var friend: FriendEntry? {
        model.people.friend(userId)
    }

    var body: some View {
        Group {
            if let friend {
                FriendDetailList(friend: friend)
            } else {
                EmptyState(title: "Not a friend anymore") {
                    IconTile(symbol: "person.crop.circle.badge.xmark", tint: TileTint.graphite, size: 52)
                } actions: {
                    EmptyView()
                }
                .background { GroundBackdrop() }
            }
        }
        .onChange(of: friend == nil) { _, gone in
            if gone {
                dismiss()
            }
        }
    }
}

private struct FriendDetailList: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var confirmingRemoval = false
    let friend: FriendEntry

    private var people: PeopleStore {
        model.people
    }

    private var terminals: [RuntimeSession] {
        model.terminals.sessions.filter { $0.ownerUserId == friend.userId }
    }

    private var rooms: [MissionEntry] {
        model.rooms.missions.filter { $0.ownerUserId == friend.userId }
    }

    var body: some View {
        List {
            Section {
                FriendHero(friend: friend)
                    .bareRow()
            }
            if friend.identityChanged {
                Section {
                    IdentityChangedCard(friend: friend)
                        .raisedRows(fill: theme.colors.cautionSoft)
                }
            }
            Section {
                if friend.isVerified {
                    SettingRow(
                        symbol: "checkmark.seal.fill",
                        tint: TileTint.green,
                        title: Text("Verified"),
                        subtitle: Text("The invite that \(friend.personName) sent you matches this identity.")
                    )
                    .raisedRows()
                    .transition(.opacity)
                } else {
                    VerifyForm(friend: friend)
                        .raisedRows()
                }
            } header: {
                GroupedListHeader(title: "Verification")
            } footer: {
                if !friend.isVerified {
                    Text("Ask \(friend.personName) for their invite in a place you trust, such as a call or a message.")
                        .appTextStyle(.footnote)
                        .foregroundStyle(theme.colors.inkFaint)
                }
            }
            Section {
                if terminals.isEmpty {
                    Text("Nothing shared with you yet")
                        .appTextStyle(.footnote)
                        .foregroundStyle(theme.colors.inkFaint)
                        .padding(.vertical, 6)
                        .raisedRows()
                } else {
                    ForEach(terminals) { session in
                        NavigationLink(value: TerminalRoute(sessionId: session.id)) {
                            SharedTerminalRow(session: session)
                        }
                        .raisedRows()
                        .accessibilityIdentifier("people.terminal.\(session.id)")
                    }
                }
            } header: {
                GroupedListHeader(title: "Shared with you", count: terminals.count)
            }
            if !rooms.isEmpty {
                Section {
                    ForEach(rooms) { room in
                        NavigationLink(value: RoomRoute(roomId: room.id)) {
                            HStack(spacing: 12) {
                                RoomSigil(key: room.id, size: 30)
                                Text(room.name)
                                    .appTextStyle(.body)
                                    .foregroundStyle(theme.colors.ink)
                                    .lineLimit(1)
                            }
                            .padding(.vertical, 3)
                        }
                        .raisedRows()
                        .accessibilityElement(children: .combine)
                        .accessibilityIdentifier("people.detail.room.\(AccessibilityIdentifier.token(room.id))")
                    }
                } header: {
                    GroupedListHeader(title: "Their rooms", count: rooms.count)
                }
            }
            Section {
                Button(role: .destructive) { confirmingRemoval = true } label: {
                    SettingRow(
                        symbol: "person.badge.minus",
                        tint: TileTint.red,
                        title: Text("Remove friend"),
                        titleColor: theme.colors.danger
                    )
                }
                .disabled(people.isBusy(friend.handle))
                .raisedRows()
                .accessibilityIdentifier("people.detail.remove")
                .confirmationDialog(Text("Remove \(friend.personName)?"), isPresented: $confirmingRemoval, titleVisibility: .visible) {
                    Button("Remove", role: .destructive) { people.remove(friend) }
                        .accessibilityIdentifier("people.detail.confirmRemove")
                } message: {
                    Text("They lose the terminals you shared. Commands they already ran stay done.")
                }
            }
        }
        .groupedListChrome()
        .navigationTitle(friend.personName)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                CopyUsernameButton(handle: friend.handle)
            }
        }
        .animation(theme.motion.spring, value: friend)
        .animation(theme.motion.spring, value: terminals.map(\.id))
        .sensoryFeedback(.success, trigger: friend.isVerified) { _, verified in verified }
    }
}

private struct FriendHero: View {
    @Environment(\.theme) private var theme
    let friend: FriendEntry

    var body: some View {
        VStack(spacing: 10) {
            PersonAvatar(name: friend.personName, key: friend.userId, size: 88)
                .accessibilityHidden(true)
                .overlay(alignment: .bottomTrailing) {
                    if friend.isVerified {
                        VerifiedSeal(size: 24)
                            .padding(3)
                            .background(theme.colors.ground, in: Circle())
                            .offset(x: 4, y: 4)
                            .transition(AnyTransition.pop)
                    }
                }
                .arrive()
            VStack(spacing: 4) {
                Text(friend.personName)
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                    .multilineTextAlignment(.center)
                Text(verbatim: "@\(friend.handle)")
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .textSelection(.enabled)
            }
            .arrive(delay: 0.04)
            statusTag
                .arrive(delay: 0.08)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 12)
        .accessibilityElement(children: .combine)
    }

    @ViewBuilder
    private var statusTag: some View {
        if friend.identityChanged {
            Tag(text: String(localized: "New identity"), symbol: "exclamationmark", tone: .caution)
        } else if friend.isVerified {
            Tag(text: String(localized: "Verified"), symbol: "checkmark", tone: .ready)
        } else {
            Tag(text: String(localized: "Not verified"))
        }
    }
}

private struct IdentityChangedCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let friend: FriendEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .top, spacing: 12) {
                IconTile(symbol: "exclamationmark.triangle.fill", tint: theme.colors.caution, size: 30)
                VStack(alignment: .leading, spacing: 3) {
                    Text("New identity")
                        .appTextStyle(.headline)
                        .foregroundStyle(theme.colors.ink)
                    Text("Their identity changed. Trust it to share with them again.")
                        .appTextStyle(.footnote)
                        .foregroundStyle(theme.colors.inkMuted)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
            HStack {
                Spacer()
                if model.people.isBusy(friend.handle) {
                    ProgressView()
                } else {
                    Button("Trust") { model.people.trust(friend) }
                        .buttonStyle(.kodosi(.primary, size: .small))
                        .accessibilityIdentifier("people.trust.\(friend.handle)")
                }
            }
        }
        .padding(.vertical, 6)
    }
}

private struct VerifyForm: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var invite = ""
    let friend: FriendEntry

    private var trimmed: String {
        invite.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                TextField("Paste the invite \(friend.personName) sent you", text: $invite)
                    .textFieldStyle(WellTextFieldStyle(radius: Radius.lg))
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .submitLabel(.done)
                    .onSubmit(verify)
                    .accessibilityIdentifier("people.verifyInvite")
                if invite.isEmpty {
                    PasteButton(payloadType: String.self) { @Sendable values in
                        guard let value = values.first else { return }
                        Task { @MainActor in invite = value }
                    }
                    .labelStyle(.iconOnly)
                    .buttonBorderShape(.capsule)
                    .tint(theme.colors.accent)
                }
            }
            HStack {
                Spacer()
                if model.people.isBusy(friend.handle) {
                    ProgressCaption(text: String(localized: "Verifying"))
                } else {
                    Button("Verify", action: verify)
                        .buttonStyle(.kodosi(.primary, size: .small))
                        .disabled(trimmed.isEmpty)
                        .accessibilityIdentifier("people.verify.\(friend.handle)")
                }
            }
        }
        .padding(.vertical, 6)
        .animation(theme.motion.snappy, value: invite.isEmpty)
    }

    private func verify() {
        guard !trimmed.isEmpty else { return }
        model.people.verify(friend, invite: trimmed)
        invite = ""
    }
}

private struct CopyUsernameButton: View {
    @State private var copied = false
    let handle: String

    var body: some View {
        Button {
            UIPasteboard.general.string = handle
            copied = true
        } label: {
            Label(copied ? String(localized: "Copied") : String(localized: "Copy username"), systemImage: copied ? "checkmark" : "doc.on.doc")
                .contentTransition(.symbolEffect(.replace))
        }
        .accessibilityIdentifier("people.detail.copyUsername")
        .sensoryFeedback(.success, trigger: copied) { _, copied in copied }
        .task(id: copied) {
            guard copied else { return }
            try? await Task.sleep(for: .seconds(1.6))
            copied = false
        }
    }
}

private struct SharedTerminalRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let session: RuntimeSession

    private var detail: String {
        if let activity = session.activity {
            return activity
        }
        let place = [session.hostName, session.missionName].compactMap(\.self).joined(separator: " · ")
        return place.isEmpty ? session.statusLabel : place
    }

    var body: some View {
        HStack(spacing: 12) {
            AgentMark(kind: session.agent, size: 32, activity: session.mark(rested: session.isConnected ? .awake : .asleep))
            VStack(alignment: .leading, spacing: 2) {
                Text(session.name)
                    .appTextStyle(.headline)
                    .foregroundStyle(theme.colors.ink)
                    .lineLimit(1)
                Text(detail)
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
                    .lineLimit(1)
            }
            Spacer(minLength: 8)
            if let sign = model.terminals.sign(of: session) {
                StatusSign(form: sign, size: 13)
            }
        }
        .padding(.vertical, 4)
        .opacity(session.canOpen ? 1 : 0.55)
        .accessibilityElement(children: .combine)
    }
}
