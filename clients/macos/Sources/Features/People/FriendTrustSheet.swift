import KodosiBridge
import KodosiDesign
import SwiftUI

struct FriendTrustSheet: View {
    @Environment(AppDependencies.self) private var deps
    let trust: FriendTrust

    var body: some View {
        FriendTrustQuestion(trust: trust, you: deps.account.selfName, cancel: close, confirm: confirm)
            .padding(28).frame(width: 420)
    }

    private func close() {
        deps.workbench.trusting = nil
    }

    private func confirm() {
        switch trust {
        case let .add(text):
            deps.people.add(text)
            deps.workbench.friendDraft = ""
        case let .accept(request):
            deps.people.accept(request)
        }
        close()
    }
}
