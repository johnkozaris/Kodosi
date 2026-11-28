import CoreTransferable
import KodosiBridge
import KodosiDesign
import SwiftUI
import UniformTypeIdentifiers

extension UTType {
    nonisolated static let kodosiTerminal = UTType(exportedAs: "com.kodosi.terminal")
}

struct TerminalDrag: Codable, Transferable {
    let sessionId: String

    /// The terminal of the drag under way in this app. A drop target reads a drag's terminal only
    /// at the drop, so a drag notes it when it starts.
    @MainActor static var current: String?

    static var transferRepresentation: some TransferRepresentation {
        CodableRepresentation(contentType: .kodosiTerminal)
    }

    @MainActor static func starting(_ sessionId: String) -> TerminalDrag {
        current = sessionId
        return TerminalDrag(sessionId: sessionId)
    }
}

struct TerminalDragPreview: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession

    var body: some View {
        HStack(spacing: 8) {
            AgentMark(session: session, size: 20, activity: session.mark(rested: .awake))
            Text(session.name).appTextStyle(.subhead).foregroundStyle(theme.colors.ink).lineLimit(1)
        }
        .padding(.leading, 8).padding(.trailing, 13).frame(height: 34)
        .raised(Radius.lg, fill: theme.colors.lifted, elevation: .floating)
    }
}

extension View {
    @ContentBuilder
    func terminalDrag(_ session: RuntimeSession, when enabled: Bool = true) -> some View {
        if enabled {
            draggable(TerminalDrag.starting(session.id)) { TerminalDragPreview(session: session) }
        } else {
            self
        }
    }

    func projectDrop(_ projectId: String, deps: AppDependencies, targeted: Binding<Bool>) -> some View {
        dropDestination(for: TerminalDrag.self) { items, _ in
            guard let id = items.first?.sessionId else { return false }
            return deps.share(id, with: projectId)
        } isTargeted: { targeted.wrappedValue = $0 }
    }
}
