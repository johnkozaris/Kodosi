import KodosiBridge
import KodosiDesign
import SwiftUI

struct NeedsYouCard: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let sign: StatusSign.Form?

    private var question: String {
        session.programStatus?.caption ?? sign?.label ?? String(localized: "Waits for you")
    }

    private var answer: LocalizedStringKey {
        switch sign {
        case .question: "Answer"
        case .key: "Sign in"
        default: "Review"
        }
    }

    private var place: String {
        [session.hostLabel, session.missionName].compactMap(\.self).joined(separator: " · ")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 12) {
                AgentMark(kind: session.agent, size: 42)
                    .padding(4)
                VStack(alignment: .leading, spacing: 2) {
                    Text(session.name)
                        .appTextStyle(.headline)
                        .foregroundStyle(theme.colors.ink)
                        .lineLimit(1)
                    HStack(spacing: 4) {
                        DeviceGlyph(label: session.hostLabel)
                        Text(place)
                            .lineLimit(1)
                    }
                    .appTextStyle(.caption)
                    .foregroundStyle(theme.colors.inkMuted)
                }
                Spacer(minLength: 8)
                if let sign {
                    StatusSign(form: sign, size: 17, backing: theme.colors.accentSoft)
                }
            }
            Text(question)
                .appTextStyle(.body)
                .foregroundStyle(theme.colors.ink)
                .lineLimit(4)
                .multilineTextAlignment(.leading)
                .id(question)
                .transition(.blurReplace)
            HStack(spacing: 10) {
                Text(answer)
                    .appTextStyle(.subhead)
                    .foregroundStyle(theme.colors.onAccent)
                    .padding(.horizontal, 18)
                    .frame(height: 36)
                    .background {
                        BezelBackground(
                            shape: RoundedRectangle(cornerRadius: KodosiButtonStyle.Size.small.radius, style: .continuous),
                            fill: theme.colors.accent
                        )
                    }
                Spacer(minLength: 8)
                Text(session.agent.label)
                    .appTextStyle(.caption)
                    .foregroundStyle(theme.colors.inkFaint)
            }
        }
        .padding(16)
        .background {
            RaisedBackground(
                shape: RoundedRectangle(cornerRadius: Radius.xl, style: .continuous),
                fill: theme.colors.lifted,
                elevation: .lifted
            )
        }
        .overlay {
            RoundedRectangle(cornerRadius: Radius.xl, style: .continuous)
                .strokeBorder(theme.colors.accent.opacity(0.55), lineWidth: 1)
        }
        .wash(trigger: question, radius: Radius.xl)
        .animation(theme.motion.soft, value: question)
        .contentShape(RoundedRectangle(cornerRadius: Radius.xl, style: .continuous))
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
        .accessibilityHint(Text("Opens the terminal"))
    }
}
