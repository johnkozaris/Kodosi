import SwiftUI

struct PeopleView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var username = ""
    @State private var pendingRemoval: FriendEntry?
    @State private var verifying: FriendEntry?
    @State private var invite = ""

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                Text("People").appTextStyle(.headingSection)
                if !deps.accountReady {
                    SignInView()
                } else {
                    HStack(spacing: 10) {
                        TextField("Friend’s username or invite", text: $username).textFieldStyle(KodosiTextFieldStyle())
                            .accessibilityIdentifier("people.username")
                        Button("Add Friend") {
                            deps.perform("friends.request.send", [
                                "username": .string(username.trimmingCharacters(in: .whitespacesAndNewlines)),
                                "requestId": .string(UUIDv7.generate()),
                            ])
                            username = ""
                        }.disabled(username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                            .buttonStyle(SolidPrimaryButtonStyle())
                            .accessibilityIdentifier("people.add")
                    }
                    HStack(spacing: 10) {
                        Button("Copy my invite") { deps.perform("friends.invite") }
                            .buttonStyle(SolidSecondaryButtonStyle())
                            .accessibilityIdentifier("people.copyInvite")
                        if deps.ownInvite != nil {
                            Image(systemName: "checkmark").foregroundStyle(theme.colors.tertiary).accessibilityLabel(Text("Invite copied"))
                        }
                    }
                    if !deps.incomingRequests.isEmpty {
                        Text("Friend requests").appTextStyle(.headingItem)
                        ForEach(deps.incomingRequests) { person in
                            HStack {
                                personName(person.displayName, handle: person.handle)
                                Spacer()
                                Button("Accept") { deps.perform("friends.request.accept", ["username": .string(person.handle)]) }
                                Button("Decline") { deps.perform("friends.request.reject", ["username": .string(person.handle)]) }
                            }
                        }
                    }
                    ForEach(deps.outgoingRequests) { person in
                        HStack {
                            personName(person.displayName, handle: person.handle)
                            Spacer()
                            Text("Request sent").appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
                            Button("Cancel") { deps.perform("friends.request.cancel", ["username": .string(person.handle)]) }
                        }
                    }
                    Text("People").appTextStyle(.headingItem)
                    if deps.friends.isEmpty {
                        Image(systemName: "person.2").font(.system(size: 40, weight: .ultraLight)).foregroundStyle(theme.colors.mutedForeground)
                            .padding(
                                .vertical,
                                24
                            )
                    }
                    ForEach(deps.friends) { friend in
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                personName(friend.displayName, handle: friend.handle)
                                Spacer()
                                if friend.identityChanged {
                                    Button("Trust") { deps.perform("friends.identity.trust", ["username": .string(friend.handle)]) }
                                        .buttonStyle(SolidPrimaryButtonStyle())
                                        .accessibilityIdentifier("people.trust.\(friend.handle)")
                                } else if friend.verified {
                                    Label("Verified", systemImage: "checkmark.seal").appTextStyle(.caption)
                                        .foregroundStyle(theme.colors.mutedForeground)
                                } else {
                                    Text("Not verified").appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
                                }
                                if !friend.verified || friend.identityChanged {
                                    Button("Verify…") { verifying = friend; invite = "" }
                                        .accessibilityIdentifier("people.verify.\(friend.handle)")
                                }
                                Button("Remove…", role: .destructive) { pendingRemoval = friend }
                            }
                            if friend.identityChanged {
                                Label("Identity changed", systemImage: "person.crop.circle.badge.exclamationmark")
                                    .appTextStyle(.caption).foregroundStyle(theme.colors.statusWaiting)
                            }
                            if verifying == friend {
                                HStack(spacing: 10) {
                                    let sender = friend.displayName.isEmpty ? friend.handle : friend.displayName
                                    TextField("Paste the invite that \(sender) sent you", text: $invite)
                                        .textFieldStyle(KodosiTextFieldStyle())
                                        .accessibilityIdentifier("people.verifyInvite")
                                    Button("Verify") {
                                        deps.perform("friends.verify", [
                                            "username": .string(friend.handle),
                                            "invite": .string(invite.trimmingCharacters(in: .whitespacesAndNewlines)),
                                        ])
                                        verifying = nil
                                    }
                                    .buttonStyle(SolidPrimaryButtonStyle())
                                    .disabled(invite.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                                    Button("Cancel") { verifying = nil }
                                }
                            }
                        }
                    }
                }
            }
            .padding(28).frame(maxWidth: 720, alignment: .leading).frame(maxWidth: .infinity, alignment: .leading)
        }
        .confirmationDialog("Remove this friend?", isPresented: Binding(get: { pendingRemoval != nil }, set: {
            if !$0 {
                pendingRemoval = nil
            }
        }), titleVisibility: .visible) {
            if let friend = pendingRemoval {
                Button("Remove \(friend.displayName)", role: .destructive) {
                    deps.perform("friends.remove", ["username": .string(friend.handle)])
                    pendingRemoval = nil
                }
            }
        } message: { Text("They will lose access to your shared sessions. Commands already executed are not undone.") }
        .onChange(of: deps.ownInvite) { _, text in
            guard let text else { return }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
        }
        .onDisappear { deps.ownInvite = nil }
    }

    private func personName(_ name: String, handle: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(name.isEmpty ? handle : name).appTextStyle(.body)
            Text("@\(handle)").appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
        }
    }
}
