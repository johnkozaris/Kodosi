import KodosiBridge
import KodosiDesign
import SwiftUI

struct ApproveDeviceSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    @State private var code = ""
    @FocusState private var codeFocused: Bool

    private var account: AccountStore {
        model.account
    }

    private var approving: Bool {
        account.approval == .approving
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                Group {
                    if case let .approved(label) = account.approval {
                        approved(label)
                    } else {
                        form
                    }
                }
                .frame(maxWidth: 460)
                .frame(maxWidth: .infinity)
                .padding(.horizontal, 24)
                .padding(.vertical, 20)
            }
            .scrollBounceBehavior(.basedOnSize)
            .background { GroundBackdrop() }
            .navigationTitle("Add a device")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(role: .close) { dismiss() }
                        .accessibilityIdentifier("settings.approve.close")
                }
            }
        }
        .presentationDetents([.large])
        .presentationBackground(theme.colors.ground)
        .animation(theme.motion.spring, value: account.approval)
        .animation(theme.motion.spring, value: account.deviceRequests)
        .sensoryFeedback(trigger: account.approval) { _, approval in
            switch approval {
            case .approved: .success
            case .failed: .error
            case .approving, nil: nil
            }
        }
        .onDisappear { account.finishApproval() }
    }

    private var form: some View {
        VStack(spacing: 24) {
            VStack(spacing: 12) {
                IconTile(symbol: "laptopcomputer.and.arrow.down", size: 58)
                    .arrive()
                Text("On the new device, choose Approve from another device. Then enter the code that it shows.")
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                    .arrive(delay: 0.04)
            }
            if !account.deviceRequests.isEmpty {
                VStack(spacing: 8) {
                    ForEach(account.deviceRequests) { request in
                        HStack(spacing: 10) {
                            BreathingDot(color: theme.colors.accent)
                            Image(systemName: DeviceGlyph.symbol(for: request.deviceLabel))
                                .foregroundStyle(theme.colors.inkMuted)
                                .accessibilityHidden(true)
                            Text("\(request.deviceLabel) is waiting")
                                .appTextStyle(.subhead)
                                .foregroundStyle(theme.colors.ink)
                            Spacer(minLength: 0)
                        }
                        .padding(.horizontal, 14)
                        .frame(minHeight: 44)
                        .background(theme.colors.accentSoft.opacity(0.6), in: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous))
                        .transition(AnyTransition.rise)
                    }
                }
            }
            codeField
                .arrive(delay: 0.08)
            if case let .failed(message) = account.approval {
                ErrorNote(message: message)
                    .accessibilityIdentifier("settings.approve.error")
                    .transition(AnyTransition.rise)
            }
            Button(action: approve) {
                HStack(spacing: 10) {
                    if approving {
                        ProgressView().tint(theme.colors.onAccent)
                    }
                    Text(approving ? "Approving" : "Approve")
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.kodosi(.primary, size: .large))
            .disabled(!DeviceCode.isComplete(code) || approving)
            .accessibilityIdentifier("settings.devices.approve")
            .arrive(delay: 0.12)
        }
    }

    private var codeField: some View {
        TextField(
            String(localized: "Code shown on the other device"),
            text: Binding(get: { code }, set: { code = DeviceCode.format($0) }),
            prompt: Text(verbatim: "XXXX-XXXX-XXXX"),
        )
        .font(.system(.title2, design: .monospaced, weight: .semibold))
        .multilineTextAlignment(.center)
        .textInputAutocapitalization(.characters)
        .autocorrectionDisabled()
        .keyboardType(.asciiCapable)
        .submitLabel(.done)
        .onSubmit(approve)
        .focused($codeFocused)
        .disabled(approving)
        .foregroundStyle(theme.colors.ink)
        .padding(.horizontal, 18)
        .frame(minHeight: 60)
        .well(Radius.xl)
        .overlay {
            RoundedRectangle(cornerRadius: Radius.xl, style: .continuous)
                .strokeBorder(theme.colors.accent.opacity(codeFocused ? 0.55 : 0), lineWidth: 1.5)
        }
        .animation(theme.motion.hover, value: codeFocused)
        .accessibilityIdentifier("settings.devices.code")
    }

    private func approved(_ label: String) -> some View {
        VStack(spacing: 22) {
            StatusSign(form: .done, size: 46)
                .frame(width: 84, height: 84)
                .background(theme.colors.readySoft, in: Circle())
                .transition(AnyTransition.pop)
            VStack(spacing: 8) {
                Text(label.isEmpty ? String(localized: "The device is approved.") : String(localized: "\(label) is approved."))
                    .appTextStyle(.title)
                    .foregroundStyle(theme.colors.ink)
                    .multilineTextAlignment(.center)
                Text("It can use your terminals and rooms now.")
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
            }
            .arrive(delay: 0.1)
            Button { dismiss() } label: {
                Text("Done").frame(maxWidth: .infinity)
            }
            .buttonStyle(.kodosi(.primary, size: .large))
            .accessibilityIdentifier("settings.approve.done")
            .arrive(delay: 0.18)
        }
        .padding(.top, 24)
    }

    private func approve() {
        guard DeviceCode.isComplete(code), !approving else { return }
        codeFocused = false
        account.approveDevice(code: code)
    }
}
