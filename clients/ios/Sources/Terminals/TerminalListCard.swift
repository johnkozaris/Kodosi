import KodosiBridge
import KodosiDesign
import SwiftUI

/// Without `showsFolder`, for a list in folders, the line names the agent rather than the folder.
struct TerminalListCard: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let sign: StatusSign.Form?
    let viewers: [AvatarStack.Person]
    var showsOwner = true
    var showsFolder = true
    var badged = true
    let open: () -> Void

    private var saysState: Bool {
        session.isTroubled || session.connectionState == .connecting || session.status != .running
    }

    var body: some View {
        TerminalCard(session: session, rested: session.canOpen ? .awake : .asleep, badged: badged, muted: !session.canOpen, open: open) {
            if saysState, model.unreachable(session) {
                Text("Offline").foregroundStyle(theme.colors.caution).lineLimit(1)
            } else {
                TerminalCaption(session: session, fallback: (showsFolder ? session.folderName : nil) ?? session.agent.label)
            }
        } accessory: {
            HStack(spacing: 8) {
                if !viewers.isEmpty {
                    AvatarStack(people: viewers, size: 20, limit: 3)
                }
                if let sign {
                    StatusSign(form: sign, size: 13).transition(AnyTransition.pop)
                } else if session.isTroubled {
                    Circle().fill(theme.colors.caution).frame(width: 6, height: 6)
                        .accessibilityLabel(Text(model.statusLabel(of: session)))
                } else if showsOwner, !session.isOwner, let owner = session.ownerName {
                    PersonAvatar(name: owner, key: session.ownerUserId, size: 20)
                }
            }
        }
        .animation(theme.motion.snappy, value: sign)
        .accessibilityValue(session.projectName.map { Text("In \($0)") } ?? Text(verbatim: ""))
    }
}
