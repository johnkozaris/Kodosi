import KodosiBridge
import KodosiDesign
import SwiftUI

struct DeviceSettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var code = ""
    @FocusState private var codeFocused: Bool
    @State private var revoke: MyDeviceEntry?
    @State private var sleeping: MyDeviceEntry?
    @State private var testing: MyDeviceEntry?
    @State private var menuDevice: String?
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
                if deps.account.localDeviceEnrolled, let reset = deps.account.otherDeviceReset {
                    StartFreshRequest(reset: reset, stop: deps.account.stopReset)
                        .padding(16).frame(maxWidth: .infinity, alignment: .leading)
                        .raised(Radius.lg)
                        .transition(AnyTransition.rise)
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
                                    tint: device.deviceId == deps.account.selfDeviceId ? TileTint.orange : TileTint.graphite)
                            {
                                power(of: device)
                                actions(for: device)
                            }
                            .wholeSubtitle()
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
                            Image(systemName: "qrcode").font(scaled: 13, weight: .semibold)
                                .foregroundStyle(theme.colors.inkFaint)
                            Text("A new phone scans a code").appTextStyle(.callout).foregroundStyle(theme.colors.ink)
                            Spacer()
                            Button("Show a code") { deps.account.offerDevice() }
                                .buttonStyle(.kodosi(.secondary))
                                .accessibilityIdentifier("settings.devices.offer")
                        }
                        .padding(.leading, 16).padding(.trailing, 6).frame(height: 46)
                        .raisedCapsule()
                        HStack(spacing: 8) {
                            Image(systemName: "laptopcomputer.and.arrow.down").font(scaled: 13, weight: .semibold)
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
                                .disabled(!DeviceCode.isComplete(code) || deps.account.approval?.isBusy == true)
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
        .animation(theme.motion.spring, value: deps.power.computers)
        .animation(theme.motion.spring, value: deps.account.deletion)
        .animation(theme.motion.spring, value: deps.account.deviceRequests)
        .animation(theme.motion.spring, value: deps.account.pendingReset)
        .confirmationDialog("Remove \(revoke?.title ?? "")?", item: $revoke, titleVisibility: .visible) { device in
            Button("Remove", role: .destructive) { deps.account.revokeDevice(device) }
        } message: { device in Text(device.removalMessage(hasRecoveryKey: deps.account.hasRecoveryKey)) }
        .confirmationDialog("Put \(sleeping?.title ?? "") to sleep?", item: $sleeping, titleVisibility: .visible) { device in
            Button("Sleep") { deps.power.sleep(device) }
                .accessibilityIdentifier("settings.devices.confirmSleep")
        } message: { device in
            Text(verbatim: [String(localized: "Its terminals pause until it wakes."), deps.power.computers[device.deviceId]?.sleepWarning]
                .compactMap(\.self).joined(separator: "\n\n"))
        }
        .confirmationDialog("Test the wake of \(testing?.title ?? "")?", item: $testing, titleVisibility: .visible) { device in
            Button("Test") { deps.power.testWake(device) }
                .accessibilityIdentifier("settings.devices.confirmTestWake")
        } message: { _ in
            Text("Kodosi puts it to sleep and wakes it again. Its terminals pause for about a minute. If it does not wake, wake it by hand.")
        }
        .confirmationDialog(Text("Approve \(confirmingLabel ?? "")?"), isPresented: Binding(get: { confirmingLabel != nil }, set: { _ in }),
                            titleVisibility: .visible)
        {
            Button("Approve") { deps.account.confirmApproval() }
                .accessibilityIdentifier("settings.devices.confirmApprove")
            Button("Cancel", role: .cancel) { deps.account.finishApproval() }
        } message: { Text("It can then use all your terminals, projects and friends. Approve it only if you hold that device.") }
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
                projects: deps.projects.entries,
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
            return String(localized: "Approves a new device when you have no other device")
        }
        let power = deps.power.computers[device.deviceId]
        let own = device.deviceId == deps.account.selfDeviceId ? String(localized: "This Mac") : nil
        let awake: String? = switch power?.keepAwake {
        case .always?: String(localized: "Always awake")
        case .never?: String(localized: "Can sleep while agents work")
        case .agents?, nil: nil
        }
        let parts = [own, awake, power?.problem ?? power?.note].compactMap(\.self)
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }

    @ContentBuilder
    private func power(of device: MyDeviceEntry) -> some View {
        let power = deps.power.computers[device.deviceId]
        if let busy = power?.busy {
            Text(busy == .waking ? "Waking…" : "Testing…").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                .accessibilityIdentifier("settings.devices.busy.\(device.deviceId)")
        } else if power?.canWake == true {
            Button("Wake") { deps.power.wake(device.deviceId) }.buttonStyle(.kodosi(.ghost, size: .small))
                .accessibilityIdentifier("settings.devices.wake.\(device.deviceId)")
        }
    }

    @ContentBuilder
    private func actions(for device: MyDeviceEntry) -> some View {
        let power = deps.power.computers[device.deviceId]
        let isSelf = device.deviceId == deps.account.selfDeviceId
        let awake = isSelf ? deps.power.keepsAwake : deps.account.computers.contains(device) && power?.busy == nil ? power?.keepAwake : nil
        let sleeps = !isSelf && deps.account.computers.contains(device) && power.map { $0.busy == nil } ?? false
        let tests = power?.canTest == true
        let removes = deps.account.canRemove(device)
        if awake != nil || sleeps || removes {
            IconButton(title: "Options for \(device.title)", symbol: "ellipsis", identifier: "settings.devices.more.\(device.deviceId)", size: 28,
                       open: menuDevice == device.deviceId)
            {
                menuDevice = menuDevice == device.deviceId ? nil : device.deviceId
            }
            .kodosiPopover(isPresented: menu(device), arrowEdge: .bottom, alignment: .trailing) {
                VStack(alignment: .leading, spacing: 2) {
                    if let awake {
                        PanelMenuHeader(title: "Stay awake")
                        ForEach(KeepAwake.allCases, id: \.self) { choice in
                            PanelMenuRow(Text(awakeTitle(choice)), icon: .symbol(awakeSymbol(choice)),
                                         identifier: "settings.devices.keepAwake.\(device.deviceId).\(choice.rawValue)")
                            {
                                if choice != awake {
                                    if isSelf {
                                        deps.power.keepAwake(choice)
                                    } else {
                                        deps.power.keepAwake(choice, on: device.deviceId)
                                    }
                                }
                            } trailing: {
                                if choice == awake {
                                    Image(systemName: "checkmark").font(scaled: 11, weight: .bold).foregroundStyle(theme.colors.accentStrong)
                                }
                            }
                            .accessibilityAddTraits(choice == awake ? .isSelected : [])
                        }
                    }
                    if sleeps {
                        if awake != nil {
                            PanelMenuDivider()
                        }
                        if tests {
                            PanelMenuRow(Text("Test wake…"), icon: .symbol("alarm"), identifier: "settings.devices.testWake.\(device.deviceId)") {
                                testing = device
                            }
                        }
                        PanelMenuRow(Text("Sleep…"), icon: .symbol("powersleep"), identifier: "settings.devices.sleep.\(device.deviceId)") {
                            sleeping = device
                        }
                    }
                    if removes {
                        if awake != nil || sleeps {
                            PanelMenuDivider()
                        }
                        PanelMenuRow(Text("Remove…"), icon: .symbol("minus.circle"), identifier: "settings.devices.remove.\(device.deviceId)",
                                     destructive: true)
                        {
                            revoke = device
                        }
                    }
                }
                .padding(6)
                .panelMenuWidth(220)
            }
        }
    }

    private func menu(_ device: MyDeviceEntry) -> Binding<Bool> {
        Binding(get: { menuDevice == device.deviceId }, set: { open in
            if !open, menuDevice == device.deviceId {
                menuDevice = nil
            }
        })
    }

    private func awakeTitle(_ choice: KeepAwake) -> String {
        switch choice {
        case .agents: String(localized: "While agents work")
        case .always: String(localized: "Always")
        case .never: String(localized: "Never")
        }
    }

    private func awakeSymbol(_ choice: KeepAwake) -> String {
        switch choice {
        case .agents: "sparkle"
        case .always: "sun.max"
        case .never: "moon.zzz"
        }
    }

    private var confirmingLabel: String? {
        if case let .confirming(label) = deps.account.approval {
            label
        } else {
            nil
        }
    }

    private func approve() {
        guard DeviceCode.isComplete(code) else { return }
        deps.account.checkDevice(code: code)
        code = ""
    }
}
