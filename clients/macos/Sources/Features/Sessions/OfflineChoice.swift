import KodosiBridge
import SwiftUI

extension View {
    func offlineChoice(_ shown: Binding<Bool>, for sessions: [RuntimeSession]) -> some View {
        modifier(OfflineChoice(shown: shown, sessions: sessions))
    }
}

private struct OfflineChoice: ViewModifier {
    @Environment(AppDependencies.self) private var deps
    @Binding var shown: Bool
    let sessions: [RuntimeSession]

    func body(content: Content) -> some View {
        content.confirmationDialog("\(sessions.first?.hostLabel ?? "") is offline", isPresented: $shown, titleVisibility: .visible) {
            Button("Hide until it's back") { deps.hide(sessions) }
                .keyboardShortcut(.defaultAction)
            Button("Close forever", role: .destructive) { deps.close(sessions) }
            Button("Cancel", role: .cancel) {}
        }
    }
}
