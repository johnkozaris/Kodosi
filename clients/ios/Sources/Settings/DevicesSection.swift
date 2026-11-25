import KodosiBridge
import KodosiDesign
import SwiftUI

struct DevicesSection: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var approving = false

    private var account: AccountStore {
        model.account
    }

    var body: some View {
        if account.localDeviceEnrolled, let reset = account.otherDeviceReset {
            StartFreshRequest(reset: reset, stop: account.stopReset)
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
                .raised(Radius.xl, fill: theme.colors.dangerSoft.opacity(theme.isDark ? 0.7 : 0.55))
        }
        if !account.orderedDevices.isEmpty {
            ListGroup(title: "Your devices") {
                ForEach(account.orderedDevices) { device in
                    DeviceRow(device: device)
                }
            }
        }
        if account.localDeviceEnrolled {
            ListGroup {
                ForEach(account.deviceRequests) { request in
                    Button { approving = true } label: {
                        ListRow(request.deviceLabel, subtitle: String(localized: "Waiting for approval"),
                                symbol: DeviceGlyph.symbol(for: request.deviceLabel), tint: TileTint.graphite)
                        {
                            StatusSign(form: .hand)
                        }
                    }
                    .buttonStyle(ListRowButtonStyle())
                    .accessibilityIdentifier("settings.devices.request.\(request.requestId)")
                }
                Button { approving = true } label: {
                    ListRow(String(localized: "Add a device"), subtitle: String(localized: "Scan or enter the code that the new device shows"),
                            symbol: "laptopcomputer.and.arrow.down", tint: theme.colors.accent)
                    {
                        Disclosure()
                    }
                }
                .buttonStyle(ListRowButtonStyle())
                .accessibilityIdentifier("settings.devices.add")
            }
            .sheet(isPresented: $approving) {
                ApproveDeviceSheet()
            }
        }
    }
}

struct DeviceRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var confirmingRemoval = false
    @State private var confirmingSleep = false
    @State private var confirmingTest = false
    let device: MyDeviceEntry

    private var account: AccountStore {
        model.account
    }

    private var isThisDevice: Bool {
        device.deviceId == account.selfDeviceId
    }

    private var removable: Bool {
        account.canRemove(device)
    }

    private var sleeps: Bool {
        account.computers.contains(device)
    }

    private var power: ComputerPower? {
        model.power.computers[device.deviceId]
    }

    private var addedAt: Date {
        Date(timeIntervalSince1970: TimeInterval(device.certIssuedAtMs) / 1000)
    }

    private var subtitle: String {
        if device.recoveryKey {
            return String(localized: "Approves a new device when you have no other device")
        }
        if isThisDevice {
            return ThisDevice.title
        }
        let awake: String? = switch power?.keepAwake {
        case .always?: String(localized: "Always awake")
        case .never?: String(localized: "Can sleep while agents work")
        case .agents?, nil: nil
        }
        let parts = [awake, power?.problem ?? power?.note].compactMap(\.self)
        return parts.isEmpty ? String(localized: "Added \(addedAt, format: .dateTime.day().month().year())") : parts.joined(separator: " · ")
    }

    @ContentBuilder
    private var powerActions: some View {
        if let power {
            Picker(selection: Binding(get: { power.keepAwake }, set: { model.power.keepAwake($0, on: device.deviceId) })) {
                Text("While agents work").tag(KeepAwake.agents)
                Text("Always").tag(KeepAwake.always)
                Text("Never").tag(KeepAwake.never)
            } label: {
                Label("Stay awake", systemImage: "sun.max")
            }
            .pickerStyle(.menu)
            .accessibilityIdentifier("settings.devices.keepAwake.\(device.deviceId)")
        }
        Button { confirmingSleep = true } label: {
            Label("Sleep…", systemImage: "powersleep")
        }
        if power?.canTest == true {
            Button { confirmingTest = true } label: {
                Label("Test wake…", systemImage: "alarm")
            }
        }
    }

    var body: some View {
        ListRow(device.title, subtitle: subtitle, symbol: device.recoveryKey ? "key.fill" : DeviceGlyph.symbol(for: device.label),
                tint: isThisDevice ? TileTint.orange : TileTint.graphite)
        {
            if let busy = power?.busy {
                Text(busy == .waking ? "Waking…" : "Testing…").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    .accessibilityIdentifier("settings.devices.busy.\(device.deviceId)")
            } else if power?.canWake == true {
                Button("Wake") { model.power.wake(device.deviceId) }
                    .buttonStyle(.kodosi(.ghost, size: .small))
                    .accessibilityLabel(Text("Wake \(device.title)"))
                    .accessibilityIdentifier("settings.devices.wake.\(device.deviceId)")
            } else if sleeps {
                Menu {
                    powerActions
                } label: {
                    IconButtonFace(symbol: "ellipsis", size: 32)
                }
                .menuStyle(.button)
                .buttonStyle(PressScaleStyle(scale: 0.9))
                .accessibilityLabel(Text("More for \(device.title)"))
                .accessibilityIdentifier("settings.devices.more.\(device.deviceId)")
            }
            if removable {
                Button("Remove…") { confirmingRemoval = true }
                    .buttonStyle(.kodosi(.ghost, size: .small))
                    .accessibilityLabel(Text("Remove \(device.title)"))
                    .accessibilityIdentifier("settings.devices.remove.\(device.deviceId)")
            }
        }
        .wholeSubtitle()
        .contextMenu {
            if sleeps, power?.busy == nil {
                powerActions
            }
            if removable {
                Button(role: .destructive) { confirmingRemoval = true } label: {
                    Label("Remove…", systemImage: "minus.circle")
                }
            }
        }
        .confirmationDialog(Text("Put \(device.title) to sleep?"), isPresented: $confirmingSleep, titleVisibility: .visible) {
            Button("Sleep") { model.power.sleep(device) }
                .accessibilityIdentifier("settings.devices.confirmSleep")
        } message: {
            Text("Its terminals pause until it wakes.")
        }
        .confirmationDialog(Text("Test the wake of \(device.title)?"), isPresented: $confirmingTest, titleVisibility: .visible) {
            Button("Test") { model.power.testWake(device) }
                .accessibilityIdentifier("settings.devices.confirmTestWake")
        } message: {
            Text("Kodosi puts it to sleep and wakes it again. Its terminals pause for about a minute. If it does not wake, wake it by hand.")
        }
        .confirmationDialog(Text("Remove \(device.title)?"), isPresented: $confirmingRemoval, titleVisibility: .visible) {
            Button("Remove", role: .destructive) { account.revokeDevice(device) }
                .accessibilityIdentifier("settings.devices.confirmRemove")
        } message: {
            Text(device.removalMessage(hasRecoveryKey: account.hasRecoveryKey))
        }
    }
}
