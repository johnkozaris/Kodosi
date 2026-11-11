import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit

struct FriendDetailView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
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
                .background(theme.colors.surface)
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
    @Environment(\.openRoute) private var openRoute
    @State private var confirmingRemoval = false
    @State private var blocking = false
    let friend: FriendEntry

    private var people: PeopleStore {
        model.people
    }

    private var terminals: [RuntimeSession] {
        model.terminals.sessions.filter { $0.ownerUserId == friend.userId && !$0.hidden }
    }

    private var projects: [ProjectEntry] {
        model.projects.entries.filter { $0.ownerUserId == friend.userId }
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                FriendHero(friend: friend)
                if friend.identityChanged {
                    IdentityChangedCard(friend: friend)
                }
                ListGroup(title: "Verification", footer: friend.isVerified ? nil
                    : "Ask \(friend.personName) for their invite in a place you trust, such as a call or a message.")
                {
                    if friend.isVerified {
                        ListRow(String(localized: "Verified"),
                                subtitle: String(localized: "The invite that \(friend.personName) sent you matches this identity."),
                                symbol: "checkmark.seal.fill", tint: TileTint.green)
                            .transition(.opacity)
                    } else {
                        VerifyForm(friend: friend)
                    }
                }
                VStack(alignment: .leading, spacing: 10) {
                    ListHeader(title: "Shared with you", count: terminals.count)
                        .padding(.leading, 4)
                    if terminals.isEmpty {
                        Text("Nothing shared with you yet")
                            .appTextStyle(.footnote)
                            .foregroundStyle(theme.colors.inkFaint)
                            .padding(.leading, 4)
                    }
                    ForEach(terminals) { session in
                        TerminalListCard(session: session, sign: model.terminals.sign(of: session), viewers: model.viewers(of: session),
                                         showsOwner: false)
                        {
                            openRoute(TerminalRoute(sessionId: session.id))
                        }
                        .accessibilityIdentifier("people.terminal.\(session.id)")
                    }
                }
                if !projects.isEmpty {
                    ListGroup(title: "Their projects") {
                        ForEach(projects) { project in
                            Button { openRoute(ProjectRoute(projectId: project.id)) } label: {
                                ListRow(title: project.name) {
                                    ProjectSigil(key: project.id, size: 28)
                                } trailing: {
                                    Disclosure()
                                }
                            }
                            .buttonStyle(ListRowButtonStyle())
                            .accessibilityIdentifier("people.detail.project.\(AccessibilityIdentifier.token(project.id))")
                        }
                    }
                }
                ListGroup {
                    Button { confirmingRemoval = true } label: {
                        ListRow(String(localized: "Remove friend"), symbol: "person.badge.minus", tint: TileTint.red,
                                titleColor: theme.colors.danger)
                    }
                    .buttonStyle(ListRowButtonStyle())
                    .disabled(people.isBusy(friend.handle))
                    .accessibilityIdentifier("people.detail.remove")
                    Button { blocking = true } label: {
                        ListRow(String(localized: "Block"), symbol: "hand.raised", tint: TileTint.red, titleColor: theme.colors.danger)
                    }
                    .buttonStyle(ListRowButtonStyle())
                    .disabled(people.isBusy(friend.handle))
                    .accessibilityIdentifier("people.detail.block")
                }
                .confirmationDialog(Text("Remove \(friend.personName)?"), isPresented: $confirmingRemoval, titleVisibility: .visible) {
                    Button("Remove", role: .destructive) { people.remove(friend) }
                        .accessibilityIdentifier("people.detail.confirmRemove")
                } message: {
                    Text("You stop sharing terminals with each other, and each of you leaves the projects that the other owns. Kodosi does not undo the commands that they already ran.")
                }
                .blockConfirmation(isPresented: $blocking, name: friend.personName) { people.block(handle: friend.handle) }
            }
            .settingsPage()
        }
        .background(theme.colors.surface)
        .navigationTitle(friend.personName)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                CopyUsernameButton(handle: friend.handle)
            }
            .sharedBackgroundVisibility(.hidden)
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
                            .background(theme.colors.surface, in: Circle())
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
                    Group {
                        if friend.inviteNeeded {
                            Text("Their identity changed. Verify it with a new invite from them.")
                        } else {
                            Text("Their identity changed. Trust it only if they told you so.")
                        }
                    }
                    .appTextStyle(.footnote)
                    .foregroundStyle(theme.colors.inkMuted)
                    .fixedSize(horizontal: false, vertical: true)
                }
            }
            if !friend.inviteNeeded {
                HStack {
                    Spacer()
                    if model.people.isBusy(friend.handle) {
                        Spinner(size: 14)
                    } else {
                        Button("Trust") { model.people.trust(friend) }
                            .buttonStyle(.kodosi(.primary, size: .small))
                            .accessibilityIdentifier("people.trust.\(friend.handle)")
                    }
                }
            }
        }
        .padding(16)
        .raised(Radius.xl)
        .overlay {
            RoundedRectangle(cornerRadius: Radius.xl, style: .continuous).strokeBorder(theme.colors.caution, lineWidth: 1.5)
        }
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
        .padding(14)
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
        IconButton(title: copied ? "Copied" : "Copy username", symbol: copied ? "checkmark" : "doc.on.doc",
                   identifier: "people.detail.copyUsername", size: 32, active: copied)
        {
            UIPasteboard.general.string = handle
            copied = true
        }
        .sensoryFeedback(.success, trigger: copied) { _, copied in copied }
        .task(id: copied) {
            guard copied else { return }
            try? await Task.sleep(for: .seconds(1.6))
            copied = false
        }
    }
}
