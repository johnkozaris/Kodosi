import KodosiDesign
import SwiftUI

struct SignInTrack: View {
    @Environment(\.theme) private var theme
    let completed: Int

    var body: some View {
        HStack(spacing: 0) {
            node(symbol: "person.fill", label: Text("You"), index: 0)
            line(reached: completed >= 2)
            node(symbol: "lock.shield.fill", label: Text(verbatim: "Kodosi"), index: 1)
            line(reached: completed >= 3)
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
                        WorkingRim(shape: Circle(), lineWidth: 2, glow: 5)
                            .padding(-2)
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

    private func line(reached: Bool) -> some View {
        Capsule()
            .fill(theme.colors.ink.opacity(0.1))
            .frame(height: 3)
            .overlay(alignment: .leading) {
                GeometryReader { geometry in
                    Capsule()
                        .fill(theme.colors.accent)
                        .frame(width: reached ? geometry.size.width : 0)
                }
            }
            .padding(.bottom, 26)
            .padding(.horizontal, -14)
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
