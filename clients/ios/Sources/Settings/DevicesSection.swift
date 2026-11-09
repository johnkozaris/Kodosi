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
                    ListRow(String(localized: "Add a device"), subtitle: String(localized: "Enter the code that the new device shows"),
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

    private var addedAt: Date {
        Date(timeIntervalSince1970: TimeInterval(device.certIssuedAtMs) / 1000)
    }

    private var subtitle: String {
        if device.recoveryKey {
            String(localized: "Approves a new device when you have no other")
        } else if isThisDevice {
            ThisDevice.title
        } else {
            String(localized: "Added \(addedAt, format: .dateTime.day().month().year())")
        }
    }

    var body: some View {
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
