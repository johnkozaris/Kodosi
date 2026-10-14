import KodosiDesign
import SwiftUI

struct SignInTrack: View {
    @Environment(\.theme) private var theme
    let completed: Int

    var body: some View {
        HStack(spacing: 0) {
            node(symbol: "person.fill", label: Text("You"), index: 0)
            line(0)
            node(symbol: "lock.shield.fill", label: Text(verbatim: "Kodosi"), index: 1)
            line(1)
            node(symbol: completed >= 3 ? "checkmark" : ThisDevice.symbol, label: Text(ThisDevice.title), index: 2)
        }
        .frame(maxWidth: 360)
        .animation(theme.motion.spring, value: completed)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text("Sign-in progress"))
        .accessibilityValue(Text("\(completed) of 3 steps complete"))
        .accessibilityIdentifier("signIn.progress")
        .onChange(of: completed) { _, steps in
            AccessibilityNotification.Announcement(String(localized: "Sign-in step \(steps) complete")).post()
        }
    }

    private func node(symbol: String, label: Text, index: Int) -> some View {
        let reached = completed > index
        let active = completed == index
        return VStack(spacing: 8) {
            Image(systemName: symbol)
                .font(.system(size: 18, weight: .semibold))
                .contentTransition(.symbolEffect(.replace))
                .foregroundStyle(reached ? theme.colors.onAccent : active ? theme.colors.ink : theme.colors.inkFaint)
                .frame(width: 48, height: 48)
                .background {
                    if reached {
                        RaisedBackground(shape: Circle(), fill: theme.colors.accent, elevation: .resting)
                            .transition(.scale(scale: 0.6).combined(with: .opacity))
                    } else {
                        WellBackground(shape: Circle())
                    }
                }
                .overlay {
                    if active {
                        CellRim(.working, lineWidth: 2.5)
                            .padding(-6)
                            .transition(.opacity)
                    }
                }
            label
                .appTextStyle(.caption)
                .foregroundStyle(reached || active ? theme.colors.ink : theme.colors.inkFaint)
                .lineLimit(1)
                .minimumScaleFactor(0.8)
        }
        .frame(width: 86)
    }

    /// The line after step `index`: it runs while the next step works, and it is lit once that step is done.
    private func line(_ index: Int) -> some View {
        CellLine(completed >= index + 2 ? .done : completed == index + 1 ? .flowing : .waiting)
            .padding(.bottom, 26)
            .padding(.horizontal, -10)
    }
}

extension AccountStore.SignInStage {
    var completedSteps: Int {
        switch self {
        case .idle, .starting, .failed: 0
        case .awaitingApproval: 1
        case .finalizing, .trustingDevice: 2
        case .signedIn: 3
        }
    }
}
