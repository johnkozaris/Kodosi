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
        Section {
            ForEach(account.deviceRequests) { request in
                Button { approving = true } label: {
                    DeviceRequestLabel(request: request)
                }
                .raisedRows(fill: theme.colors.accentSoft.opacity(theme.isDark ? 0.7 : 0.55))
                .accessibilityIdentifier("settings.devices.request.\(request.requestId)")
            }
            ForEach(account.orderedDevices) { device in
                DeviceRow(device: device)
                    .raisedRows()
            }
            if account.localDeviceEnrolled {
                Button { approving = true } label: {
                    SettingRow(
                        symbol: "laptopcomputer.and.arrow.down",
                        tint: theme.colors.accent,
                        title: Text("Add a device"),
                        subtitle: Text("Enter the code that the new device shows"),
                    ) {
                        if !account.deviceRequests.isEmpty {
                            CountBadge(count: account.deviceRequests.count)
                                .transition(AnyTransition.pop)
                        }
                    }
                }
                .raisedRows()
                .accessibilityIdentifier("settings.devices.add")
                .sheet(isPresented: $approving) {
                    ApproveDeviceSheet()
                }
            }
        } header: {
            GroupedListHeader(title: "Your devices", count: account.devices.count { !$0.recoveryKey })
        }
    }
}

private struct DeviceRequestLabel: View {
    @Environment(\.theme) private var theme
    let request: DeviceLinkRequest

    var body: some View {
        HStack(spacing: 14) {
            BreathingDot(color: theme.colors.accent, size: 9)
                .frame(width: 30)
            Text("\(request.deviceLabel) is waiting. Enter its code.")
                .appTextStyle(.body)
                .foregroundStyle(theme.colors.ink)
                .multilineTextAlignment(.leading)
            Spacer(minLength: 8)
            Image(systemName: "chevron.right")
                .font(.footnote.weight(.semibold))
                .foregroundStyle(theme.colors.inkFaint)
        }
        .padding(.vertical, 6)
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

    private var title: String {
        device.title
    }

    private var addedAt: Date {
        Date(timeIntervalSince1970: TimeInterval(device.certIssuedAtMs) / 1000)
    }

    private var subtitle: Text {
        if device.recoveryKey {
            Text("Approves a new device when you have no other")
        } else if isThisDevice {
            Text(ThisDevice.title)
        } else {
            Text("Added \(addedAt, format: .dateTime.day().month().year())")
        }
    }

    var body: some View {
        SettingRow(
            symbol: device.recoveryKey ? "key.fill" : DeviceGlyph.symbol(for: device.label),
            tint: isThisDevice ? TileTint.orange : TileTint.graphite,
            title: Text(title),
            subtitle: subtitle,
        ) {
            if removable {
                Button("Remove") { confirmingRemoval = true }
                    .buttonStyle(.kodosi(.ghost, size: .small))
                    .accessibilityLabel(Text("Remove \(title)"))
                    .accessibilityIdentifier("settings.devices.remove.\(device.deviceId)")
            }
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            if removable {
                Button { confirmingRemoval = true } label: {
                    Label("Remove", systemImage: "minus.circle")
                }
                .tint(theme.colors.danger)
            }
        }
        .contextMenu {
            if removable {
                Button(role: .destructive) { confirmingRemoval = true } label: {
                    Label("Remove…", systemImage: "minus.circle")
                }
            }
        }
        .confirmationDialog(Text("Remove \(title)?"), isPresented: $confirmingRemoval, titleVisibility: .visible) {
            Button("Remove", role: .destructive) { account.revokeDevice(device) }
                .accessibilityIdentifier("settings.devices.confirmRemove")
        } message: {
            Text(device.removalMessage(hasRecoveryKey: account.hasRecoveryKey))
        }
    }
}
