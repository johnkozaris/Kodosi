import KodosiBridge
import SwiftUI

public struct StartFreshRequest: View {
    @Environment(\.theme) private var theme
    let reset: DeviceReset
    let stop: () -> Void

    public init(reset: DeviceReset, stop: @escaping () -> Void) {
        self.reset = reset
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
            Button(action: stop) { Text("Stop it", bundle: .module) }
                .buttonStyle(.kodosi(.primary))
                .accessibilityIdentifier("devices.reset.stop")
        }
    }
}
