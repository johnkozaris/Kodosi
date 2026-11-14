import KodosiBridge
import KodosiDesign
import SwiftUI

struct ApproveDeviceSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    @State private var code = ""
    @State private var scanning = false
    @FocusState private var codeFocused: Bool

    private var account: AccountStore {
        model.account
    }

    private var approving: Bool {
        account.approval?.isBusy == true
    }

    private var confirmingLabel: String? {
        if case let .confirming(label) = account.approval {
            label
        } else {
            nil
        }
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                Group {
                    if case let .approved(label) = account.approval {
                        approved(label)
                    } else if scanning {
                        scanner
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
            .background(theme.colors.surface)
            .navigationTitle("Add a device")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    IconButton(title: "Close", symbol: "xmark", identifier: "settings.approve.close", size: 32) { dismiss() }
                }
                .sharedBackgroundVisibility(.hidden)
            }
        }
        .presentationDetents([.large])
        .kodosiSheet()
        .animation(theme.motion.spring, value: account.approval)
        .animation(theme.motion.spring, value: account.deviceRequests)
        .animation(theme.motion.spring, value: scanning)
        .sensoryFeedback(trigger: account.approval) { _, approval in
            switch approval {
            case .approved: .success
            case .failed: .error
            case .checking, .confirming, .approving, nil: nil
            }
        }
        .confirmationDialog(Text("Approve \(confirmingLabel ?? "")?"), isPresented: Binding(get: { confirmingLabel != nil }, set: { _ in }),
                            titleVisibility: .visible)
        {
            Button("Approve") { account.confirmApproval() }
                .accessibilityIdentifier("settings.devices.confirmApprove")
            Button("Cancel", role: .cancel) { account.finishApproval() }
        } message: {
            Text("It can then use all your terminals, projects and friends. Approve it only if you hold that device.")
        }
        .onDisappear { account.finishApproval() }
    }

    private var form: some View {
        VStack(spacing: 24) {
            VStack(spacing: 12) {
                IconTile(symbol: "laptopcomputer.and.arrow.down", size: 58)
                    .arrive()
                Text("On the new device, choose Approve from another device. Then scan the code that it shows, or enter it.")
                    .appTextStyle(.callout)
                    .foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
                    .arrive(delay: 0.04)
            }
            if !account.deviceRequests.isEmpty {
                ListGroup {
                    ForEach(account.deviceRequests) { request in
                        ListRow(request.deviceLabel, subtitle: String(localized: "Waiting for approval"),
                                symbol: DeviceGlyph.symbol(for: request.deviceLabel), tint: TileTint.graphite)
                        {
                            StatusSign(form: .hand)
                        }
                    }
                }
                .transition(AnyTransition.rise)
            }
            Button { scanning = true } label: {
                Label("Scan the code", systemImage: "qrcode.viewfinder")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.kodosi(.primary, size: .large))
            .disabled(approving)
            .accessibilityIdentifier("settings.approve.scan")
            .arrive(delay: 0.08)
            codeField
                .arrive(delay: 0.12)
            if case let .failed(message) = account.approval {
                ErrorNote(message: message)
                    .accessibilityIdentifier("settings.approve.error")
                    .transition(AnyTransition.rise)
            }
            Button(action: approve) {
                Text(approving ? "Approving" : "Approve")
                    .shimmer(approving)
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.kodosi(.secondary, size: .large))
            .disabled(!DeviceCode.isComplete(code) || approving)
            .accessibilityIdentifier("settings.devices.approve")
            .arrive(delay: 0.16)
        }
    }

    /// The scanner stays in this sheet: a sheet that closes at the moment of the scan can
    /// stop the confirmation from coming up.
    private var scanner: some View {
        VStack(spacing: 16) {
            CodeScanView(scan: CodeScan(
                hint: "Point the camera at the code that the new device shows.", other: "This is not the code of a new device.",
                identifier: "settings.approve.scan", accepts: { DeviceCode.scanned(in: $0, for: .approve) },
            ) { scanned in
                scanning = false
                account.checkDevice(code: scanned)
            })
            .frame(height: 440)
            Button { scanning = false } label: { Text("Enter the code") }
                .buttonStyle(.kodosi(.ghost))
                .accessibilityIdentifier("settings.approve.enter")
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
        .textFieldStyle(.plain)
        .tint(theme.colors.accent)
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
                Text("It can use your terminals and projects now.")
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
        account.checkDevice(code: code)
    }
}
