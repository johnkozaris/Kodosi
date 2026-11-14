import KodosiBridge
import KodosiDesign
import SwiftUI

struct DeviceOfferSheet: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        Group {
            switch deps.account.offer {
            case let .approved(label): approved(label)
            case .ended: ended(nil)
            case let .failed(message): ended(message)
            case .requesting, .shown, nil: code
            }
        }
        .padding(28).frame(width: 400)
        .animation(theme.motion.spring, value: deps.account.offer)
    }

    private var code: some View {
        VStack(spacing: 20) {
            VStack(spacing: 10) {
                Text("Add a phone").appTextStyle(.title).foregroundStyle(theme.colors.ink)
                Text("On the new phone, open Kodosi and sign in. Then choose Scan the code on your computer.")
                    .appTextStyle(.callout).foregroundStyle(theme.colors.inkMuted)
                    .multilineTextAlignment(.center).fixedSize(horizontal: false, vertical: true)
            }
            ScanCodeCard {
                if case let .shown(rows) = deps.account.offer {
                    ScanCode(rows: rows)
                        .accessibilityElement()
                        .accessibilityLabel(Text("QR code for the new phone"))
                        .accessibilityAddTraits(.isImage)
                        .accessibilityIdentifier("deviceOffer.qr")
                        .transition(.opacity)
                } else {
                    Spinner(size: 18)
                }
            }
            .frame(width: 240)
            ProgressCaption(text: String(localized: "Waiting for the phone"))
            Button("Cancel") { deps.account.endOffer() }
                .buttonStyle(.kodosi(.ghost)).keyboardShortcut(.cancelAction)
                .accessibilityIdentifier("deviceOffer.cancel")
        }
    }

    private func approved(_ label: String) -> some View {
        VStack(spacing: 20) {
            StatusSign(form: .done, size: 46)
                .frame(width: 84, height: 84)
                .background(theme.colors.readySoft, in: Circle())
                .transition(AnyTransition.pop)
            VStack(spacing: 8) {
                Text(label.isEmpty ? String(localized: "The device is approved.") : String(localized: "\(label) is approved."))
                    .appTextStyle(.title).foregroundStyle(theme.colors.ink).multilineTextAlignment(.center)
                    .accessibilityIdentifier("deviceOffer.approved")
                Text("It can use your terminals and projects now.")
                    .appTextStyle(.callout).foregroundStyle(theme.colors.inkMuted).multilineTextAlignment(.center)
            }
            Button("Done") { deps.account.endOffer() }
                .buttonStyle(.kodosi(.primary)).keyboardShortcut(.defaultAction)
                .accessibilityIdentifier("deviceOffer.done")
        }
    }

    private func ended(_ message: String?) -> some View {
        VStack(spacing: 20) {
            if let message {
                ErrorNote(message: message).accessibilityIdentifier("deviceOffer.error")
            } else {
                Text("This code no longer works.").appTextStyle(.title).foregroundStyle(theme.colors.ink)
                    .multilineTextAlignment(.center).accessibilityIdentifier("deviceOffer.ended")
            }
            HStack(spacing: 8) {
                Button("Close") { deps.account.endOffer() }
                    .buttonStyle(.kodosi(.ghost)).keyboardShortcut(.cancelAction)
                    .accessibilityIdentifier("deviceOffer.close")
                Button("Show a new code") { deps.account.offerDevice() }
                    .buttonStyle(.kodosi(.primary)).keyboardShortcut(.defaultAction)
                    .accessibilityIdentifier("deviceOffer.again")
            }
        }
    }
}
