import KodosiBridge
import KodosiDesign
import SwiftUI

struct ReportTarget: Equatable {
    let userId: String
    let handle: String
    let name: String
}

struct ReportSheet: View {
    @Environment(AppDependencies.self) private var deps
    let person: ReportTarget

    var body: some View {
        ReportForm(name: person.name, key: person.userId, send: send, cancel: close, reported: reported)
            .padding(28).frame(width: 420)
    }

    private func send(_ note: String) async throws {
        try await deps.people.report(handle: person.handle, note: note)
    }

    private func close() {
        deps.workbench.reporting = nil
    }

    private func reported() {
        close()
        deps.doneMessage = String(localized: "Kodosi got your report.")
    }
}
