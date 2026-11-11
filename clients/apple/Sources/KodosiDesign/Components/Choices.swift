import SwiftUI

public struct ProjectChoice: View {
    @Environment(\.theme) private var theme
    let name: String
    let key: String
    let selected: Bool

    public init(name: String, key: String, selected: Bool) {
        self.name = name
        self.key = key
        self.selected = selected
    }

    public var body: some View {
        HStack(spacing: 7) {
            ProjectSigil(key: key, size: 18)
            Text(name).appTextStyle(.footnote).fontWeight(.medium).lineLimit(1)
            if selected {
                Image(systemName: "checkmark").font(.system(size: 9, weight: .bold))
                    .foregroundStyle(theme.colors.accentStrong).transition(AnyTransition.pop)
            }
        }
        .foregroundStyle(selected ? theme.colors.ink : theme.colors.inkMuted)
        .padding(.leading, 5).padding(.trailing, 10).controlHeight(28)
        .background {
            if selected {
                RaisedBackground(shape: Capsule(), fill: theme.colors.lifted, elevation: .resting)
            } else {
                WellBackground(shape: Capsule())
            }
        }
        .touchTarget()
    }
}

public struct FriendChoice: View {
    @Environment(\.theme) private var theme
    let name: String
    let key: String
    let selected: Bool
    let changed: Bool
    let blocked: Bool
    let ring: Color?

    /// A `changed` friend has a new identity, and a `blocked` one cannot be chosen until you trust
    /// it. The `ring` is the ground under the face.
    public init(name: String, key: String, selected: Bool, changed: Bool = false, blocked: Bool = false, ring: Color? = nil) {
        self.name = name
        self.key = key
        self.selected = selected
        self.changed = changed
        self.blocked = blocked
        self.ring = ring
    }

    public var body: some View {
        let ring = ring ?? theme.colors.raised
        VStack(spacing: 5) {
            PersonAvatar(name: name, key: key, size: 40)
                .overlay { Circle().strokeBorder(theme.colors.accent, lineWidth: selected ? 2.5 : 0).padding(-4) }
                .overlay(alignment: .bottomTrailing) {
                    if selected {
                        Image(systemName: "checkmark").font(.system(size: 8, weight: .heavy)).foregroundStyle(theme.colors.onAccent)
                            .frame(width: 16, height: 16).background(theme.colors.accent, in: Circle())
                            .overlay { Circle().stroke(ring, lineWidth: 2) }
                            .offset(x: 5, y: 5).transition(AnyTransition.pop)
                    } else if changed {
                        Circle().fill(theme.colors.caution).frame(width: 12, height: 12)
                            .overlay { Circle().stroke(ring, lineWidth: 2) }.offset(x: 3, y: 3)
                    }
                }
            Text(name.split(separator: " ").first.map(String.init) ?? name)
                .appTextStyle(.caption).foregroundStyle(selected ? theme.colors.ink : theme.colors.inkMuted).lineLimit(1)
        }
        #if os(iOS)
        .frame(minWidth: 60)
        #else
        .frame(width: 60)
        #endif
        .padding(.top, 4).opacity(blocked ? 0.45 : 1)
    }
}
