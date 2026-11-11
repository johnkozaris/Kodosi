import KodosiBridge
import SwiftUI

extension FriendTrust {
    var face: String {
        guard case let .accept(request) = self else { return handle ?? "?" }
        return request.personName
    }

    var firstName: String? {
        guard case let .accept(request) = self else { return name }
        return request.personName.split(separator: " ").first.map(String.init) ?? request.personName
    }
}

public struct FriendTrustQuestion: View {
    @Environment(\.theme) private var theme
    let trust: FriendTrust
    let you: String
    let cancel: () -> Void
    let confirm: () -> Void

    public init(trust: FriendTrust, you: String, cancel: @escaping () -> Void, confirm: @escaping () -> Void) {
        self.trust = trust
        self.you = you
        self.cancel = cancel
        self.confirm = confirm
    }

    public var body: some View {
        VStack(spacing: 22) {
            TrustArt(you: you, them: trust.face, key: trust.handle)
            VStack(spacing: 12) {
                Tag(text: String(localized: "Stop and read", bundle: .module), symbol: "hand.raised.fill", tone: .caution)
                Group {
                    if let name = trust.name {
                        Text("Do you trust \(name) completely?", bundle: .module)
                    } else {
                        Text("Do you trust this person completely?", bundle: .module)
                    }
                }
                .appTextStyle(.title).foregroundStyle(theme.colors.ink).multilineTextAlignment(.center)
                Text("A friend can type in each terminal that you share with them, as you, with your rights on that computer.", bundle: .module)
                    .appTextStyle(.body).foregroundStyle(theme.colors.inkMuted).multilineTextAlignment(.center)
                Text("Add only a person that you would let use your computer.", bundle: .module)
                    .appTextStyle(.body).fontWeight(.semibold).foregroundStyle(theme.colors.ink).multilineTextAlignment(.center)
            }
            .fixedSize(horizontal: false, vertical: true)
            HStack(spacing: 8) {
                Button(action: cancel) { Text("Cancel", bundle: .module) }
                    .keyboardShortcut(.cancelAction).buttonStyle(.kodosi(.ghost, size: .large))
                    .accessibilityIdentifier("friend.trust.cancel")
                Button(action: confirm) {
                    Text(trust.firstName.map { String(localized: "I trust \($0)", bundle: .module) } ?? String(localized: "I trust them", bundle: .module))
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.kodosi(.primary, size: .large))
                .accessibilityIdentifier("friend.trust.confirm")
            }
        }
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
