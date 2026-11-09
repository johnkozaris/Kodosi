import KodosiBridge
import SwiftUI

/// What an approved device shows while another device of the account waits to start fresh.
public struct StartFreshRequest: View {
    @Environment(\.theme) private var theme
    @State private var allowing = false
    let reset: DeviceReset
    let allow: () -> Void
    let stop: () -> Void

    public init(reset: DeviceReset, allow: @escaping () -> Void, stop: @escaping () -> Void) {
        self.reset = reset
        self.allow = allow
        self.stop = stop
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label {
                Text("\(reset.deviceLabel) asked to start fresh", bundle: .module)
            } icon: {
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(theme.colors.danger)
            }
            .appTextStyle(.headline)
            .foregroundStyle(theme.colors.ink)
            Text("""
            If nobody stops it, it can start fresh after \(reset.readyAt, format: .dateTime.weekday(.wide).hour().minute()), \
            and all your devices lose access. Stop it unless you asked for it.
            """, bundle: .module)
                .appTextStyle(.footnote)
                .foregroundStyle(theme.colors.inkMuted)
                .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 10) {
                Button(action: stop) { Text("Stop it", bundle: .module) }
                    .buttonStyle(.kodosi(.primary))
                    .accessibilityIdentifier("devices.reset.stop")
                Button { allowing = true } label: { Text("Allow now…", bundle: .module) }
                    .buttonStyle(.kodosi(.ghost))
                    .accessibilityIdentifier("devices.reset.allow")
            }
        }
        .confirmationDialog(
            Text("Let \(reset.deviceLabel) start fresh now?", bundle: .module), isPresented: $allowing, titleVisibility: .visible,
        ) {
            Button(role: .destructive, action: allow) { Text("Allow", bundle: .module) }
        } message: {
            Text("All your devices lose access, and your friends confirm your identity again. Allow it only if you asked for it.", bundle: .module)
        }
    }
}
