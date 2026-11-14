import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit

struct PeopleRoute: Hashable {}

struct FriendRoute: Hashable {
    let userId: String
}

struct IncomingRequestRow: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var blocking = false
    @State private var reporting = false
    let request: FriendRequestEntry
    let accept: () -> Void

    private var people: PeopleStore {
        model.people
    }

    var body: some View {
        FriendRequestCard(request: request, busy: people.isBusy(request.handle), identifier: "people.request.\(request.handle)") {
            people.decline(request)
        } accept: {
            accept()
        }
        .swipeActions(edge: .leading, allowsFullSwipe: true) {
            Button(action: accept) {
                Label("Accept", systemImage: "checkmark")
            }
            .tint(theme.colors.accent)
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            Button { people.decline(request) } label: {
                Label("Not now", systemImage: "xmark")
            }
            .tint(TileTint.graphite)
            Button { blocking = true } label: {
                Label("Block", systemImage: "hand.raised")
            }
            .tint(theme.colors.danger)
            Button { reporting = true } label: {
                Label("Report", systemImage: "flag")
            }
            .tint(theme.colors.danger)
        }
        .contextMenu {
            Button(action: accept) {
                Label("Accept", systemImage: "checkmark")
            }
            Button { people.decline(request) } label: {
                Label("Not now", systemImage: "xmark")
            }
            Button(role: .destructive) { blocking = true } label: {
                Label("Block…", systemImage: "hand.raised")
            }
            Button(role: .destructive) { reporting = true } label: {
                Label("Report…", systemImage: "flag")
            }
        }
        .blockConfirmation(isPresented: $blocking, name: request.personName) { people.block(handle: request.handle) }
        .reportSheet($reporting, handle: request.handle, name: request.personName, key: request.userId)
    }
}

struct OutgoingRequestRow: View {
    @Environment(AppModel.self) private var model
    let request: FriendRequestEntry

    private var people: PeopleStore {
        model.people
    }

    var body: some View {
        OutgoingRequestCard(request: request, busy: people.isBusy(request.handle), identifier: "people.request.cancel.\(request.handle)") {
            people.cancel(request)
        }
        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
            Button { people.cancel(request) } label: {
                Label("Cancel request", systemImage: "xmark")
            }
            .tint(TileTint.graphite)
        }
        .contextMenu {
            Button { people.cancel(request) } label: {
                Label("Cancel request", systemImage: "xmark")
            }
            Button { UIPasteboard.general.string = request.handle } label: {
                Label("Copy username", systemImage: "doc.on.doc")
            }
        }
    }
}

/// The member stays after its dialog or sheet closes, so that the closing view keeps the name.
struct MemberAction {
    var member: ProjectMemberEntry?
    var blocking = false
    var reporting = false

    mutating func block(_ member: ProjectMemberEntry) {
        self.member = member
        blocking = true
    }

    mutating func report(_ member: ProjectMemberEntry) {
        self.member = member
        reporting = true
    }
}

extension View {
    func friendTrust(_ trust: Binding<FriendTrust?>, confirm: @escaping (FriendTrust) -> Void) -> some View {
        modifier(FriendTrustAsk(trust: trust, confirm: confirm))
    }

    func reportSheet(_ shown: Binding<Bool>, handle: String, name: String, key: String) -> some View {
        modifier(ReportAsk(shown: shown, handle: handle, name: name, key: key))
    }

    func memberAction(_ action: Binding<MemberAction>, people: PeopleStore) -> some View {
        let member = action.wrappedValue.member
        return blockConfirmation(isPresented: action.blocking, name: member?.personName ?? "") {
            if let member {
                people.block(handle: member.handle)
            }
        }
        .reportSheet(action.reporting, handle: member?.handle ?? "", name: member?.personName ?? "", key: member?.userId ?? "")
    }
}

private struct ReportAsk: ViewModifier {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Binding var shown: Bool
    let handle: String
    let name: String
    let key: String

    func body(content: Content) -> some View {
        content.sheet(isPresented: $shown) {
            ScrollView {
                ReportForm(name: name, key: key) { note in
                    try await model.people.report(handle: handle, note: note)
                } cancel: {
                    shown = false
                } reported: {
                    shown = false
                    model.notices.confirm(String(localized: "Kodosi got your report."))
                }
                .padding(.horizontal, 24)
                .padding(.vertical, 28)
                .frame(maxWidth: 460)
                .frame(maxWidth: .infinity)
            }
            .scrollBounceBehavior(.basedOnSize)
            .scrollDismissesKeyboard(.interactively)
            .background(theme.colors.surface)
            .presentationDetents([.medium, .large])
            .kodosiSheet()
        }
    }
}

private struct FriendTrustAsk: ViewModifier {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Binding var trust: FriendTrust?
    let confirm: (FriendTrust) -> Void

    private var shown: Binding<Bool> {
        Binding(get: { trust != nil }, set: { shown in
            if !shown {
                trust = nil
            }
        })
    }

    func body(content: Content) -> some View {
        content.sheet(isPresented: shown) {
            if let current = trust {
                ScrollView {
                    FriendTrustQuestion(trust: current, you: model.account.selfName) {
                        trust = nil
                    } confirm: {
                        trust = nil
                        confirm(current)
                    }
                    .padding(.horizontal, 24)
                    .padding(.vertical, 28)
                    .frame(maxWidth: 460)
                    .frame(maxWidth: .infinity)
                }
                .scrollBounceBehavior(.basedOnSize)
                .background(theme.colors.surface)
                .presentationDetents([.medium, .large])
                .kodosiSheet()
            }
        }
    }
}
