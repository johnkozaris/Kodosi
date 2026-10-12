import KodosiBridge
import KodosiDesign
import SwiftUI

struct TerminalRow: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let sign: StatusSign.Form?
    let viewers: [AvatarStack.Person]

    var body: some View {
        HStack(spacing: 12) {
            AgentMark(kind: session.agent, size: 36, activity: session.mark(rested: session.isConnected ? .awake : .asleep))
                .padding(3)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: 6) {
                    Text(session.name)
                        .appTextStyle(.headline)
                        .foregroundStyle(theme.colors.ink)
                        .lineLimit(1)
                        .contentTransition(.interpolate)
                    if let room = session.missionName {
                        RoomTag(name: room, key: session.missionId ?? room)
                    }
                }
                TerminalCaption(session: session)
                    .appTextStyle(.footnote)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if !viewers.isEmpty {
                AvatarStack(people: viewers, size: 22, limit: 3, ring: theme.colors.raised)
            }
            if let sign {
                StatusSign(form: sign, size: 13)
                    .transition(AnyTransition.pop)
            }
        }
        .padding(.vertical, 6)
        .opacity(session.canOpen ? 1 : 0.62)
        .animation(theme.motion.snappy, value: sign)
        .accessibilityElement(children: .combine)
    }
}

struct TerminalCaption: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession

    private var place: String {
        if session.isOwner {
            return session.folderName ?? session.agent.label
        }
        return [session.ownerName, session.hostName].compactMap(\.self).joined(separator: " · ")
    }

    var body: some View {
        HStack(spacing: 5) {
            if session.isTroubled || session.connectionState == .connecting {
                Text(session.statusLabel)
                    .foregroundStyle(session.isTroubled ? theme.colors.caution : theme.colors.inkFaint)
                    .shimmer(session.connectionState == .connecting)
            } else if session.status == .reconnecting {
                Text("Reconnecting")
                    .foregroundStyle(theme.colors.caution)
                    .shimmer()
            } else if session.status == .closing {
                Text(session.statusLabel)
                    .foregroundStyle(theme.colors.inkFaint)
            } else {
                activity
            }
        }
        .lineLimit(1)
        .animation(theme.motion.soft, value: session.activity)
        .animation(theme.motion.snappy, value: session.progress)
    }

    @ViewBuilder
    private var activity: some View {
        let words = session.activity ?? place
        ZStack(alignment: .leading) {
            Text(words)
                .foregroundStyle(session.activity == nil ? theme.colors.inkFaint : theme.colors.inkMuted)
                .shimmer(session.isWorking)
                .id(words)
                .transition(.blurReplace)
        }
        if let progress = session.progress {
            Text(Double(progress) / 100, format: .percent)
                .monospacedDigit()
                .foregroundStyle(theme.colors.inkMuted)
                .contentTransition(.numericText(value: Double(progress)))
                .layoutPriority(1)
        }
    }
}

struct RoomTag: View {
    @Environment(\.theme) private var theme
    let name: String
    let key: String

    var body: some View {
        HStack(spacing: 4) {
            RoomSigil(name: name, key: key, size: 13)
            Text(name)
                .lineLimit(1)
        }
        .appTextStyle(.caption2)
        .foregroundStyle(theme.colors.inkMuted)
        .padding(.leading, 3)
        .padding(.trailing, 7)
        .frame(height: 19)
        .background(theme.colors.ink.opacity(0.07), in: Capsule())
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Text("In \(name)"))
    }
}
