import KodosiBridge
import KodosiDesign
import SwiftUI

struct DeviceSettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var code = ""
    @FocusState private var codeFocused: Bool
    @State private var revoke: MyDeviceEntry?
    @State private var signingOut = false
    @State private var deletingAccount = false
    @State private var replacingRecoveryKey = false

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            if deps.account.userId == nil {
                SignInView().frame(height: 380)
            } else {
                VStack(alignment: .leading, spacing: 8) {
                    HStack(spacing: 14) {
                        PersonAvatar(name: deps.account.selfName, size: 52, isSelf: true)
                        VStack(alignment: .leading, spacing: 3) {
                            Text(deps.account.selfName).appTextStyle(.title).foregroundStyle(theme.colors.ink)
                            Label("Encrypted end to end", systemImage: "lock.fill").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        }
                        Spacer()
                        Button("Sign out…") { signingOut = true }.accessibilityIdentifier("settings.account.signOut")
                    }
                    .padding(16).raised(Radius.xl)
                    if let kept {
                        Text(kept).appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
                            .padding(.leading, 4).fixedSize(horizontal: false, vertical: true)
                            .accessibilityIdentifier("settings.account.kept")
                    }
                }
                if !deps.account.localDeviceEnrolled {
                    VStack(spacing: 14) {
                        Text("Trust this Mac").appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                        DeviceTrustView(trust: deps.deviceTrust)
                    }
                    .padding(20).frame(maxWidth: .infinity)
                    .raised(Radius.xl)
                }
                if !deps.account.orderedDevices.isEmpty {
                    ListGroup(title: "Your devices") {
                        ForEach(deps.account.orderedDevices) { device in
                            ListRow(device.title,
                                    subtitle: subtitle(device),
                                    symbol: device.recoveryKey ? "key.fill" : DeviceGlyph.symbol(for: device.label),
                                    tint: device.deviceId == deps.account.selfDeviceId ? TileTint.copper : TileTint.graphite)
                            {
                                if deps.account.canRemove(device) {
                                    Button("Remove…") { revoke = device }.buttonStyle(.kodosi(.ghost, size: .small))
                                        .accessibilityIdentifier("settings.devices.remove.\(device.deviceId)")
                                }
                            }
                        }
                    }
                }
                if deps.account.localDeviceEnrolled {
                    VStack(alignment: .leading, spacing: 10) {
                        Text("Add a device").appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted).padding(.leading, 4)
                        if !deps.account.deviceRequests.isEmpty {
                            ListGroup {
                                ForEach(deps.account.deviceRequests) { request in
                                    ListRow(request.deviceLabel, subtitle: String(localized: "Waiting for approval"),
                                            symbol: DeviceGlyph.symbol(for: request.deviceLabel), tint: TileTint.graphite)
                                    {
                                        StatusSign(form: .hand)
                                    }
                                }
                            }
                            .transition(AnyTransition.rise)
                        }
                        HStack(spacing: 8) {
                            Image(systemName: "laptopcomputer.and.arrow.down").font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(theme.colors.inkFaint)
                            TextField(
                                "Code shown on the other device",
                                text: Binding(get: { code }, set: { code = DeviceCode.format($0) }),
                            )
                            .textFieldStyle(.plain).appTextStyle(.callout).onSubmit(approve)
                            .focused($codeFocused)
                            .accessibilityIdentifier("settings.devices.code")
                            Button("Approve", action: approve)
                                .buttonStyle(.kodosi(.primary))
                                .disabled(!DeviceCode.isComplete(code) || deps.account.approval == .approving)
                                .accessibilityIdentifier("settings.devices.approve")
                        }
                        .padding(.leading, 16).padding(.trailing, 6).frame(height: 46)
                        .raisedCapsule()
                    }
                }
                if deps.account.localDeviceEnrolled {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("If you lose all your devices").appTextStyle(.subhead).foregroundStyle(theme.colors.inkMuted).padding(.leading, 4)
                        HStack(spacing: 12) {
                            Text("A recovery key approves a new device when you have no other device.")
                                .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                            Spacer()
                            Button(deps.account.hasRecoveryKey ? String(localized: "Make a new key…") : String(localized: "Make a recovery key")) {
                                if deps.account.hasRecoveryKey {
                                    replacingRecoveryKey = true
                                } else {
                                    deps.account.createRecoveryKey()
                                }
                            }
                            .buttonStyle(.kodosi(.secondary))
                            .accessibilityIdentifier("settings.devices.recoveryKey")
                        }
                        .padding(.horizontal, 16).padding(.vertical, 12).raised(Radius.lg)
                    }
                }
                if let deletion = deps.account.deletion {
                    HStack(spacing: 10) {
                        BreathingDot(color: theme.colors.danger)
                        Text("Confirm the deletion in your browser.").appTextStyle(.body).foregroundStyle(theme.colors.ink)
                        Spacer()
                        if case let .confirming(page?) = deletion {
                            Button("Open Page") { deps.openExternalURL(page) }
                                .buttonStyle(.kodosi(.secondary, size: .small))
                                .accessibilityIdentifier("settings.account.deletionPage")
                        }
                        Button("Cancel") { deps.account.cancelAccountDeletion() }
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
        .animation(theme.motion.spring, value: deps.account.devices)
        .animation(theme.motion.spring, value: deps.account.deletion)
        .animation(theme.motion.spring, value: deps.account.deviceRequests)
        .confirmationDialog("Remove \(revoke?.title ?? "")?", isPresented: Binding(get: { revoke != nil }, set: {
            if !$0 {
                revoke = nil
            }
        }), titleVisibility: .visible) {
            if let device = revoke {
                Button("Remove", role: .destructive) { deps.account.revokeDevice(device); revoke = nil }
            }
        } message: { Text(revoke?.removalMessage(hasRecoveryKey: deps.account.hasRecoveryKey) ?? "") }
        .confirmationDialog("Make a new recovery key?", isPresented: $replacingRecoveryKey, titleVisibility: .visible) {
            Button("Make a new key") { deps.account.createRecoveryKey() }
        } message: { Text("The recovery key that you have now stops working.") }
        .confirmationDialog("Sign out?", isPresented: $signingOut, titleVisibility: .visible) {
            Button("Sign out", role: .destructive) { deps.account.signOut() }
        } message: { Text("Sharing ends. Terminals on this Mac keep running.") }
        .confirmationDialog("Delete your Kodosi account?", isPresented: $deletingAccount, titleVisibility: .visible) {
            Button("Delete Account", role: .destructive) { deps.account.deleteAccount() }
        } message: {
            Text(deps.account.deletionSummary(
                rooms: deps.missions,
                terminals: String(localized: "Terminals on this Mac keep running. You cannot undo this."),
            ))
        }
        .onAppear {
            if deps.account.userId != nil {
                deps.perform("devices.refresh")
            }
            if deps.workbench.focusesDeviceCode {
                codeFocused = true
                deps.workbench.focusesDeviceCode = false
            }
        }
    }

    private var kept: String? {
        switch deps.account.credentialStore {
        case "keychain": String(localized: "Kodosi keeps your sign-in in the Keychain.")
        case "privateFile": deps.account.credentialFolder.map { String(localized: "Kodosi keeps your sign-in in private files in \($0).") }
        default: nil
        }
    }

    private func subtitle(_ device: MyDeviceEntry) -> String? {
        if device.recoveryKey {
            String(localized: "Approves a new device when you have no other")
        } else if device.deviceId == deps.account.selfDeviceId {
            String(localized: "This Mac")
        } else {
            nil
        }
    }

    private func approve() {
        guard DeviceCode.isComplete(code) else { return }
        deps.account.approveDevice(code: code)
        code = ""
    }
}
