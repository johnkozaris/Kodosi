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

    var body: some View {
        if let power, !isThisDevice {
            NavigationLink(value: ComputerRoute(deviceId: device.deviceId)) {
                ListRow(device.title, subtitle: subtitle, symbol: DeviceGlyph.symbol(for: device.label), tint: TileTint.graphite) {
                    if let busy = power.busy {
                        Text(busy == .waking ? "Waking…" : "Testing…").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                            .accessibilityIdentifier("settings.devices.busy.\(device.deviceId)")
                    }
                    Disclosure()
                }
                .wholeSubtitle()
            }
            .buttonStyle(ListRowButtonStyle())
            .accessibilityIdentifier("settings.devices.open.\(device.deviceId)")
        } else {
            ListRow(device.title, subtitle: subtitle, symbol: device.recoveryKey ? "key.fill" : DeviceGlyph.symbol(for: device.label),
                    tint: isThisDevice ? TileTint.orange : TileTint.graphite)
            {
                if removable {
                    Button("Remove…") { confirmingRemoval = true }
                        .buttonStyle(.kodosi(.ghost, size: .small))
                        .accessibilityLabel(Text("Remove \(device.title)"))
                        .accessibilityIdentifier("settings.devices.remove.\(device.deviceId)")
                }
            }
            .wholeSubtitle()
            .contextMenu {
                if removable {
                    Button(role: .destructive) { confirmingRemoval = true } label: {
                        Label("Remove…", systemImage: "minus.circle")
                    }
                }
            }
            .confirmationDialog(Text("Remove \(device.title)?"), isPresented: $confirmingRemoval, titleVisibility: .visible) {
                Button("Remove", role: .destructive) { account.revokeDevice(device) }
                    .accessibilityIdentifier("settings.devices.confirmRemove")
            } message: {
                Text(device.removalMessage(hasRecoveryKey: account.hasRecoveryKey))
            }
        }
    }
}
