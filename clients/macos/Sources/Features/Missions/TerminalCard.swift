import KodosiBridge
import KodosiDesign
import SwiftUI

struct TerminalCard<Detail: View, Accessory: View>: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let session: RuntimeSession
    let rested: AgentMark.Activity
    let open: () -> Void
    @ViewBuilder let detail: Detail
    @ViewBuilder let accessory: Accessory

    var body: some View {
        Button(action: open) {
            HStack(spacing: 12) {
                AgentMark(session: session, size: 34, activity: session.mark(rested: rested))
                VStack(alignment: .leading, spacing: 3) {
                    Text(session.name).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
                    detail.appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                }
                Spacer(minLength: 0)
                accessory
            }
            .padding(.horizontal, 14).frame(height: 68)
            .raised(Radius.lg, fill: hovered ? theme.colors.lifted : theme.colors.raised, elevation: hovered ? .lifted : .resting)
            .offset(y: hovered ? -1 : 0)
        }
        .buttonStyle(PressScaleStyle(scale: 0.98))
        .onHover { hovered = $0 }
        .animation(theme.motion.spring, value: hovered)
    }
}
