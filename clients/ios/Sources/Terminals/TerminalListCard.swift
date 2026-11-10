import KodosiBridge
import KodosiDesign
import SwiftUI

/// A terminal in a list: its card, with the people who have it open, and what it needs or that it
/// is in trouble. Without `showsFolder`, for a list in folders, its line names its agent rather
/// than its folder.
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

    /// Whether the line under the name says how the terminal connects rather than what it does.
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
        .accessibilityValue(session.missionName.map { Text("In \($0)") } ?? Text(verbatim: ""))
    }
}
