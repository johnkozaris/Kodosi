import SwiftUI

struct DeviceSettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var code = ""
    @State private var revoke: MyDeviceEntry?
    @State private var signingOut = false
    @State private var deletingAccount = false
    @State private var replacingRecoveryKey = false

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            if deps.userId == nil {
                SignInView().frame(height: 380)
            } else {
                HStack(spacing: 14) {
                    PersonAvatar(name: deps.selfName, size: 52, isSelf: true)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(deps.selfName).appTextStyle(.title).foregroundStyle(theme.colors.ink)
                        Label("Encrypted end to end", systemImage: "lock.fill").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    }
                    Spacer()
                    Button("Sign out…") { signingOut = true }.accessibilityIdentifier("settings.account.signOut")
                }
                .padding(16).raised(Radius.xl)
                if !deps.localDeviceEnrolled {
                    VStack(spacing: 14) {
                        Text("Trust this Mac").appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                        DeviceTrustView(trust: deps.deviceTrust)
                    }
                    .padding(20).frame(maxWidth: .infinity)
                    .raised(Radius.xl)
                    .overlay {
                        RoundedRectangle(cornerRadius: Radius.xl, style: .continuous).strokeBorder(theme.colors.accent.opacity(0.6), lineWidth: 1.5)
                    }
                }
                ListGroup(title: "Your devices") {
                    ForEach(deps.devices) { device in
                        ListRow(device.recoveryKey ? String(localized: "Recovery key") : device.label,
                                subtitle: subtitle(device),
                                symbol: device.recoveryKey ? "key.fill" : DeviceGlyph.symbol(for: device.label),
                                tint: device.deviceId == deps.selfDeviceId ? TileTint.orange : TileTint.graphite)
                        {
                            if deps.localDeviceEnrolled, device.deviceId != deps.selfDeviceId {
                                Button("Remove…") { revoke = device }.buttonStyle(.kodosi(.ghost, size: .small))
                                    .accessibilityIdentifier("settings.devices.remove.\(device.deviceId)")
                            }
                        }
                    }
                }
                if deps.localDeviceEnrolled {
                    VStack(alignment: .leading, spacing: 10) {
                        Text("Add a device").appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted).padding(.leading, 4)
                        ForEach(deps.deviceRequests) { request in
                            HStack(spacing: 10) {
                                BreathingDot(color: theme.colors.accent)
                                Text("\(request.deviceLabel) is waiting. Enter its code.").appTextStyle(.body).foregroundStyle(theme.colors.ink)
                            }
                            .padding(.horizontal, 14).frame(height: 40).frame(maxWidth: .infinity, alignment: .leading)
                            .background(theme.colors.accentSoft.opacity(0.6), in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
                            .transition(AnyTransition.rise)
                        }
                        HStack(spacing: 8) {
                            Image(systemName: "laptopcomputer.and.arrow.down").font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(theme.colors.inkFaint)
                            TextField("Code shown on the other device", text: $code)
                                .textFieldStyle(.plain).appTextStyle(.callout).onSubmit(approve)
                                .accessibilityIdentifier("settings.devices.code")
                            Button("Approve", action: approve)
                                .buttonStyle(.kodosi(.primary))
                                .disabled(code.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                                .accessibilityIdentifier("settings.devices.approve")
                        }
                        .padding(.leading, 16).padding(.trailing, 6).frame(height: 46)
                        .raisedCapsule()
                    }
                }
                if deps.localDeviceEnrolled {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("If you lose all your devices").appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted).padding(.leading, 4)
                        HStack(spacing: 12) {
                            Text("A recovery key approves a new device when you have no other device.")
                                .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                            Spacer()
                            Button(hasRecoveryKey ? String(localized: "Make a new key…") : String(localized: "Make a recovery key")) {
                                if hasRecoveryKey {
                                    replacingRecoveryKey = true
                                } else {
                                    deps.perform("devices.recovery.create")
                                }
                            }
                            .buttonStyle(.kodosi(.secondary))
                            .accessibilityIdentifier("settings.devices.recoveryKey")
                        }
                        .padding(.horizontal, 16).padding(.vertical, 12).raised(Radius.lg)
                    }
                }
                if let deletion = deps.accountDeletion {
                    HStack(spacing: 10) {
                        BreathingDot(color: theme.colors.danger)
                        Text("Confirm the deletion in your browser.").appTextStyle(.body).foregroundStyle(theme.colors.ink)
                        Spacer()
                        if let page = deletion.page {
                            Button("Open Page") { deps.openExternalURL(page) }
                                .buttonStyle(.kodosi(.secondary, size: .small))
                                .accessibilityIdentifier("settings.account.deletionPage")
                        }
                        Button("Cancel") { deps.cancelAccountDeletion() }
                            .buttonStyle(.kodosi(.ghost, size: .small))
                            .accessibilityIdentifier("settings.account.deletionCancel")
                    }
                    .padding(.horizontal, 16).frame(height: 46).raised(Radius.lg)
                    .transition(AnyTransition.rise)
                } else {
                    Button("Delete my account…") { deletingAccount = true }
                        .buttonStyle(.kodosi(.ghost, size: .small)).foregroundStyle(theme.colors.danger)
                        .accessibilityIdentifier("settings.account.delete")
                }
            }
        }
        .animation(theme.motion.spring, value: deps.devices)
        .animation(theme.motion.spring, value: deps.accountDeletion)
        .animation(theme.motion.spring, value: deps.deviceRequests)
        .confirmationDialog("Remove \(revoke?.label ?? "")?", isPresented: Binding(get: { revoke != nil }, set: {
            if !$0 {
                revoke = nil
            }
        }), titleVisibility: .visible) {
            if let device = revoke {
                Button("Remove", role: .destructive) { deps.perform("devices.revoke", ["deviceId": .string(device.deviceId)]); revoke = nil }
            }
        } message: { Text(removalMessage) }
        .confirmationDialog("Make a new recovery key?", isPresented: $replacingRecoveryKey, titleVisibility: .visible) {
            Button("Make a new key") { deps.perform("devices.recovery.create") }
        } message: { Text("The recovery key that you have now stops working.") }
        .sheet(isPresented: Binding(get: { deps.newRecoveryKey != nil }, set: {
            if !$0 {
                deps.newRecoveryKey = nil
            }
        })) {
            RecoveryKeySheet(key: deps.newRecoveryKey ?? "") { deps.newRecoveryKey = nil }
        }
        .confirmationDialog("Sign out?", isPresented: $signingOut, titleVisibility: .visible) {
            Button("Sign out", role: .destructive) { deps.perform("auth.logout") }
        } message: { Text("Sharing ends. Terminals on this Mac keep running.") }
        .confirmationDialog("Delete your Kodosi account?", isPresented: $deletingAccount, titleVisibility: .visible) {
            Button("Delete Account", role: .destructive) { deps.perform("auth.deleteAccount") }
        } message: {
            Text(deletionSummary)
        }
        .onAppear {
            if deps.userId != nil {
                deps.perform("devices.refresh")
            }
        }
    }

    private var deletionSummary: String {
        let owned = deps.missions.filter { $0.ownerUserId == deps.userId }.map(\.name)
        var parts = [String(localized: "Your devices, friends and sharing go away. What you wrote in other rooms stays there.")]
        if !owned.isEmpty {
            parts.append(String(localized: "These rooms close for everyone in them: \(owned.formatted(.list(type: .and)))."))
        }
        parts.append(String(localized: "Terminals on this Mac keep running. This cannot be undone."))
        return parts.joined(separator: "\n\n")
    }

    private var hasRecoveryKey: Bool {
        deps.devices.contains(where: \.recoveryKey)
    }

    private var removalMessage: String {
        if revoke?.recoveryKey == true {
            return String(localized: "The recovery key stops working. You can make a new one.")
        }
        let removal = String(localized: "It disconnects. Approve it again to use it. Commands it already ran stay done.")
        return hasRecoveryKey ? removal + " " + String(localized: "If it was lost or stolen, also make a new recovery key.") : removal
    }

    private func subtitle(_ device: MyDeviceEntry) -> String? {
        if device.recoveryKey {
            String(localized: "Approves a new device when you have no other")
        } else if device.deviceId == deps.selfDeviceId {
            String(localized: "This Mac")
        } else {
            nil
        }
    }

    private func approve() {
        let value = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty else { return }
        deps.perform("devices.link.approve", ["code": .string(value)])
        code = ""
    }
}
