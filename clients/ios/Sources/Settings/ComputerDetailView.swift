import KodosiBridge
import KodosiDesign
import SwiftUI

struct ComputerRoute: Hashable {
    let deviceId: String
}

/// One computer of the person: how it wakes, when it stays awake, and what to do with it.
struct ComputerDetailView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    @State private var confirmingSleep = false
    @State private var confirmingTest = false
    @State private var confirmingRemoval = false
    let deviceId: String

    private var account: AccountStore {
        model.account
    }

    private var device: MyDeviceEntry? {
        account.devices.first { $0.deviceId == deviceId }
    }

    private var power: ComputerPower? {
        model.power.computers[deviceId]
    }

    var body: some View {
        Group {
            if let device {
                page(device)
            } else {
                theme.colors.surface
            }
        }
        .onChange(of: device == nil) { _, gone in
            if gone {
                dismiss()
            }
        }
    }

    private func page(_ device: MyDeviceEntry) -> some View {
        let online = account.computers.contains(device)
        return ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                hero(device, online: online)
                if let power {
                    ListGroup(title: "Stay awake", footer: online ? footer(power.keepAwake, name: device.title) : "You can change this while it is online.") {
                        ForEach(KeepAwake.allCases, id: \.self) { choice in
                            Button {
                                if choice != power.keepAwake {
                                    model.power.keepAwake(choice, on: deviceId)
                                }
                            } label: {
                                ListRow(title(choice), symbol: symbol(choice), tint: tint(choice)) {
                                    if choice == power.keepAwake {
                                        Image(systemName: "checkmark").font(scaled: 14, weight: .semibold).foregroundStyle(theme.colors.accentStrong)
                                            .transition(AnyTransition.pop)
                                    }
                                }
                            }
                            .buttonStyle(ListRowButtonStyle())
                            .disabled(!online || power.busy != nil)
                            .accessibilityAddTraits(choice == power.keepAwake ? .isSelected : [])
                            .accessibilityIdentifier("computer.keepAwake.\(choice.rawValue)")
                        }
                    }
                }
                if online, power?.busy == nil {
                    ListGroup {
                        if power?.canTest == true {
                            Button { confirmingTest = true } label: {
                                ListRow(String(localized: "Test wake…"), symbol: "alarm", tint: TileTint.teal)
                            }
                            .buttonStyle(ListRowButtonStyle())
                            .accessibilityIdentifier("computer.testWake")
                            .confirmationDialog(Text("Test the wake of \(device.title)?"), isPresented: $confirmingTest, titleVisibility: .visible) {
                                Button("Test") { model.power.testWake(device) }
                                    .accessibilityIdentifier("computer.confirmTestWake")
                            } message: {
                                Text("Kodosi puts it to sleep and wakes it again. Its terminals pause for about a minute. If it does not wake, wake it by hand.")
                            }
                        }
                        Button { confirmingSleep = true } label: {
                            ListRow(String(localized: "Sleep…"), symbol: "powersleep", tint: TileTint.graphite)
                        }
                        .buttonStyle(ListRowButtonStyle())
                        .accessibilityIdentifier("computer.sleep")
                        .confirmationDialog(Text("Put \(device.title) to sleep?"), isPresented: $confirmingSleep, titleVisibility: .visible) {
                            Button("Sleep") { model.power.sleep(device) }
                                .accessibilityIdentifier("computer.confirmSleep")
                        } message: {
                            Text(verbatim: [String(localized: "Its terminals pause until it wakes."), power?.sleepWarning].compactMap(\.self)
                                .joined(separator: "\n\n"))
                        }
                    }
                }
                if account.canRemove(device) {
                    ListGroup {
                        Button { confirmingRemoval = true } label: {
                            ListRow(String(localized: "Remove…"), symbol: "minus.circle", tint: TileTint.red, titleColor: theme.colors.danger)
                        }
                        .buttonStyle(ListRowButtonStyle())
                        .accessibilityIdentifier("computer.remove")
                        .confirmationDialog(Text("Remove \(device.title)?"), isPresented: $confirmingRemoval, titleVisibility: .visible) {
                            Button("Remove", role: .destructive) { account.revokeDevice(device) }
                                .accessibilityIdentifier("computer.confirmRemove")
                        } message: {
                            Text(device.removalMessage(hasRecoveryKey: account.hasRecoveryKey))
                        }
                    }
                }
            }
            .settingsPage()
        }
        .background(theme.colors.surface)
        .navigationTitle(device.title)
        .navigationBarTitleDisplayMode(.inline)
        .animation(theme.motion.spring, value: power)
        .animation(theme.motion.spring, value: online)
    }

    private func hero(_ device: MyDeviceEntry, online: Bool) -> some View {
        let status = [online ? nil : String(localized: "Offline"), power?.problem ?? power?.note].compactMap(\.self).joined(separator: ". ")
        return VStack(spacing: 8) {
            IconTile(symbol: DeviceGlyph.symbol(for: device.label), tint: TileTint.graphite, size: 56)
            Text(device.title).appTextStyle(.title).foregroundStyle(theme.colors.ink).multilineTextAlignment(.center)
            if !status.isEmpty {
                Text(status).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                    .accessibilityIdentifier("computer.status")
            }
            if let busy = power?.busy {
                Text(busy == .waking ? "Waking…" : "Testing…").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                    .accessibilityIdentifier("computer.busy")
            } else if power?.canWake == true {
                Button("Wake") { model.power.wake(deviceId) }
                    .buttonStyle(.kodosi(.primary))
                    .padding(.top, 4)
                    .accessibilityIdentifier("computer.wake")
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.top, 8)
    }

    private func footer(_ choice: KeepAwake, name: String) -> LocalizedStringKey {
        switch choice {
        case .agents:
            "\(name) does not go to sleep by itself while an agent works, or for a few minutes after. An agent that waits for you does not keep it awake."
        case .always:
            "\(name) does not go to sleep by itself while Kodosi runs. On battery, it stays awake only while agents work."
        case .never:
            "\(name) can go to sleep while an agent works. The agent then pauses until it wakes."
        }
    }

    private func title(_ choice: KeepAwake) -> String {
        switch choice {
        case .agents: String(localized: "While agents work")
        case .always: String(localized: "Always")
        case .never: String(localized: "Never")
        }
    }

    private func symbol(_ choice: KeepAwake) -> String {
        switch choice {
        case .agents: "sparkle"
        case .always: "sun.max"
        case .never: "moon.zzz"
        }
    }

    private func tint(_ choice: KeepAwake) -> Color {
        switch choice {
        case .agents: TileTint.blue
        case .always: TileTint.orange
        case .never: TileTint.purple
        }
    }
}
