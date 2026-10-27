import KodosiBridge
import KodosiDesign
import SwiftUI

enum FriendTrust: Equatable {
    case add(String)
    case accept(FriendRequestEntry)

    var handle: String? {
        switch self {
        case let .accept(request):
            return request.handle
        case let .add(text):
            let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
            let parts = text.split(separator: ":", omittingEmptySubsequences: false)
            let handle = parts.count == 3 && parts[0] == "kodosi" ? String(parts[1]) : text.hasPrefix("@") ? String(text.dropFirst()) : text
            let allowed = CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "._-"))
            guard !handle.isEmpty, handle.count <= 64, handle.unicodeScalars.allSatisfy(allowed.contains) else { return nil }
            return handle
        }
    }

    var name: String? {
        switch self {
        case let .accept(request): request.personName
        case .add: handle.map { "@\($0)" }
        }
    }
}

struct FriendTrustSheet: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let trust: FriendTrust

    var body: some View {
        VStack(spacing: 22) {
            TrustArt(you: deps.account.selfName, them: trust.face, key: trust.handle)
            VStack(spacing: 12) {
                Tag(text: String(localized: "Stop and read"), symbol: "hand.raised.fill", tone: .caution)
                Group {
                    if let name = trust.name {
                        Text("Do you trust \(name) completely?")
                    } else {
                        Text("Do you trust this person completely?")
                    }
                }
                .appTextStyle(.title).foregroundStyle(theme.colors.ink).multilineTextAlignment(.center)
                Text("A friend can type in each terminal that you share with them, as you, with your rights on that computer.")
                    .appTextStyle(.body).foregroundStyle(theme.colors.inkMuted).multilineTextAlignment(.center)
                Text("Add only a person that you would let use your computer.")
                    .appTextStyle(.body).fontWeight(.semibold).foregroundStyle(theme.colors.ink).multilineTextAlignment(.center)
            }
            .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 8) {
                Button("Cancel", action: close)
                    .keyboardShortcut(.cancelAction).buttonStyle(.kodosi(.ghost, size: .large))
                    .accessibilityIdentifier("friend.trust.cancel")
                Button(action: confirm) {
                    Text(trust.firstName.map { String(localized: "I trust \($0)") } ?? String(localized: "I trust them")).frame(maxWidth: .infinity)
                }
                .buttonStyle(.kodosi(.primary, size: .large))
                .accessibilityIdentifier("friend.trust.confirm")
            }
        }
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

private extension FriendTrust {
    var face: String {
        guard case let .accept(request) = self else { return handle ?? "?" }
        return request.personName
    }

    var firstName: String? {
        guard case let .accept(request) = self else { return name }
        return request.personName.split(separator: " ").first.map(String.init) ?? request.personName
    }
}

private struct TrustArt: View {
    @Environment(\.theme) private var theme
    let you: String
    let them: String
    let key: String?

    var body: some View {
        HStack(spacing: 10) {
            PersonAvatar(name: you, size: 44, isSelf: true)
            line
            HStack(spacing: 6) {
                Image(systemName: "chevron.right").font(.system(size: 11, weight: .heavy)).foregroundStyle(theme.colors.inkFaint)
                CursorBlock(width: 7, height: 13, blink: .typing)
            }
            .padding(.horizontal, 12).frame(width: 86, height: 52, alignment: .leading)
            .background(theme.colors.terminal, in: RoundedRectangle(cornerRadius: Radius.md, style: .continuous))
            .overlay { RoundedRectangle(cornerRadius: Radius.md, style: .continuous).strokeBorder(theme.colors.hairline, lineWidth: 0.5) }
            line
            PersonAvatar(name: them, key: key, size: 44)
        }
        .accessibilityHidden(true)
    }

    private var line: some View {
        Capsule().fill(theme.colors.hairline).frame(width: 18, height: 2)
    }
}
