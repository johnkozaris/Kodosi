import KodosiBridge
import KodosiDesign
import SwiftUI

/// What the person writes to Dosi, and the computer that answers it.
struct DosiComposer: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    var focused: FocusState<Bool>.Binding
    @State private var sent = 0

    private var typed: Bool {
        !model.dosi.draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var stops: Bool {
        model.dosi.busy && !typed
    }

    private var canSend: Bool {
        typed && model.dosiConnected && model.dosiComputer != nil
    }

    var body: some View {
        @Bindable var dosi = model.dosi
        let connected = model.dosiConnected
        let computer = model.dosiComputer
        VStack(alignment: .leading, spacing: 8) {
            Group {
                if !connected {
                    ProgressCaption(text: String(localized: "Connecting…"))
                        .accessibilityIdentifier("dosi.connecting")
                } else if let computer {
                    DosiComputerMenu(computer: computer)
                } else {
                    Label("Open Kodosi on one of your computers to talk with Dosi.", systemImage: "laptopcomputer")
                        .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        .padding(.horizontal, 12).padding(.vertical, 9)
                        .raised(Radius.md, fill: theme.colors.raised)
                        .accessibilityIdentifier("dosi.noComputer")
                }
            }
            .transition(.opacity)
            HStack(alignment: .bottom, spacing: 4) {
                DosiFace(mood: dosi.mood(listening: focused.wrappedValue), size: 24)
                    .frame(width: 34, height: 34)
                TextField("Ask Dosi", text: $dosi.draft, axis: .vertical)
                    .textFieldStyle(.plain)
                    .tint(theme.colors.accent)
                    .lineLimit(1 ... 6)
                    .appTextStyle(.body)
                    .foregroundStyle(theme.colors.ink)
                    .focused(focused)
                    .padding(.vertical, 7)
                    .disabled(!connected)
                    .accessibilityIdentifier("dosi.input")
                    .onKeyPress(.return, phases: .down) { press in
                        guard press.modifiers.isDisjoint(with: [.shift, .option]) else { return .ignored }
                        if canSend {
                            send()
                        }
                        return .handled
                    }
                if stops {
                    StopKey { model.dosi.stop() }
                        .disabled(dosi.state == .stopping)
                        .transition(AnyTransition.pop)
                } else {
                    SendKey(enabled: canSend, size: 34, action: send)
                        .accessibilityLabel(Text("Send"))
                        .accessibilityIdentifier("dosi.send")
                        .transition(AnyTransition.pop)
                }
            }
            .composerField(focused: focused.wrappedValue)
        }
        .padding(.horizontal, 12)
        .padding(.top, 6)
        .padding(.bottom, 8)
        .frame(maxWidth: 760)
        .frame(maxWidth: .infinity)
        .animation(theme.motion.snappy, value: stops)
        .animation(theme.motion.snappy, value: canSend)
        .animation(theme.motion.hover, value: focused.wrappedValue)
        .animation(theme.motion.soft, value: connected)
        .animation(theme.motion.soft, value: computer == nil)
        .sensoryFeedback(.impact(weight: .light), trigger: sent)
    }

    private func send() {
        guard canSend else { return }
        model.sendToDosi()
        sent += 1
    }
}

/// The computer that answers Dosi here, and the menu of the person's computers.
private struct DosiComputerMenu: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let computer: DosiComputer

    var body: some View {
        let away = model.dosiAway(computer)
        Menu {
            Picker(selection: Binding(get: { computer.device }, set: { model.dosiDevice = $0 })) {
                ForEach(model.dosiComputers) { choice in
                    let away = model.dosiAway(choice)
                    Label {
                        name(choice, away: away)
                    } icon: {
                        Image(systemName: away ? "wifi.slash" : DeviceGlyph.symbol(for: choice.name))
                    }
                    .tag(choice.device)
                }
            } label: {
                EmptyView()
            }
        } label: {
            HStack(spacing: 6) {
                DeviceGlyph(label: computer.name).imageScale(.small)
                Text(verbatim: computer.name).lineLimit(1)
                if away {
                    Image(systemName: "wifi.slash").imageScale(.small).foregroundStyle(theme.colors.inkFaint)
                }
                Image(systemName: "chevron.up.chevron.down").imageScale(.small).foregroundStyle(theme.colors.inkFaint)
            }
            .appTextStyle(.footnote).fontWeight(.medium)
            .foregroundStyle(theme.colors.ink)
            .padding(.horizontal, 12).frame(minHeight: 30)
            .background { BezelBackground(shape: Capsule(), fill: theme.buttonFace(), bezel: .quiet) }
            .touchTarget()
        }
        .menuStyle(.button)
        .buttonStyle(PressScaleStyle())
        .accessibilityLabel(Text("Computer"))
        .accessibilityValue(name(computer, away: away))
        .accessibilityIdentifier("dosi.computer")
    }

    private func name(_ computer: DosiComputer, away: Bool) -> Text {
        away ? Text("\(computer.name) is offline") : Text(verbatim: computer.name)
    }
}

/// The key that stops the turn that Dosi works on: a square on a quiet face.
private struct StopKey: View {
    @Environment(\.theme) private var theme
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            RoundedRectangle(cornerRadius: 2.5, style: .continuous).fill(theme.colors.ink).frame(width: 10, height: 10)
                .frame(width: 34, height: 34)
                .background { BezelBackground(shape: Circle(), fill: theme.buttonFace(), bezel: .quiet) }
                .touchTarget()
        }
        .buttonStyle(PressScaleStyle())
        .accessibilityLabel(Text("Stop"))
        .accessibilityIdentifier("dosi.stop")
    }
}
