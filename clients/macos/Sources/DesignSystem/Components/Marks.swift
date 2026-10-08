import SwiftUI

struct PersonAvatar: View {
    @Environment(\.theme) private var theme
    let name: String
    var key: String?
    var size: CGFloat = 28
    var isSelf = false
    var ring: Color?
    var online = false

    private var initials: String {
        let letters = name.split(separator: " ").prefix(2).compactMap(\.first).map(String.init).joined().uppercased()
        return letters.isEmpty ? "·" : letters
    }

    private var tint: Color {
        isSelf ? theme.colors.accent : PersonTint.color(for: key ?? name)
    }

    var body: some View {
        Text(initials)
            .font(.system(size: size * 0.38, weight: .semibold, design: .rounded))
            .foregroundStyle(isSelf ? theme.colors.onAccent : .white)
            .frame(width: size, height: size)
            .background {
                Circle().fill(LinearGradient(
                    colors: [tint.mix(with: .white, by: 0.18), tint, tint.mix(with: .black, by: 0.16)],
                    startPoint: .topLeading, endPoint: .bottomTrailing
                ))
            }
            .overlay { Circle().strokeBorder(.white.opacity(0.22), lineWidth: 0.5) }
            .overlay {
                if let ring {
                    Circle().stroke(ring, lineWidth: 2).padding(-1)
                }
            }
            .overlay(alignment: .bottomTrailing) {
                if online {
                    Circle().fill(theme.colors.ready).frame(width: size * 0.3, height: size * 0.3)
                        .overlay { Circle().stroke(ring ?? theme.colors.surface, lineWidth: 1.5) }
                        .offset(x: 1, y: 1)
                }
            }
            .accessibilityLabel(name)
    }
}

struct AvatarStack: View {
    @Environment(\.theme) private var theme
    struct Person: Identifiable {
        let id: String
        let name: String
        var isSelf = false
    }

    let people: [Person]
    var size: CGFloat = 24
    var limit = 4
    var ring: Color?

    var body: some View {
        HStack(spacing: -size * 0.28) {
            ForEach(people.prefix(limit)) { person in
                PersonAvatar(name: person.name, key: person.id, size: size, isSelf: person.isSelf, ring: ring ?? theme.colors.surface)
                    .transition(AnyTransition.pop)
            }
            if people.count > limit {
                Text("+\(people.count - limit)")
                    .font(.system(size: size * 0.38, weight: .semibold, design: .rounded))
                    .foregroundStyle(theme.colors.inkMuted)
                    .frame(width: size, height: size)
                    .background(theme.colors.well, in: Circle())
                    .overlay { Circle().stroke(ring ?? theme.colors.surface, lineWidth: 2).padding(-1) }
            }
        }
        .animation(theme.motion.snappy, value: people.map(\.id))
    }
}

struct AgentMark: View {
    enum Activity { case asleep, awake, working }

    @Environment(\.theme) private var theme
    let kind: AgentKind
    var size: CGFloat = 22
    var activity: Activity = .awake

    init(program: String?, size: CGFloat = 22, activity: Activity = .awake) {
        kind = AgentKind(program: program)
        self.size = size
        self.activity = activity
    }

    init(kind: AgentKind, size: CGFloat = 22, activity: Activity = .awake) {
        self.kind = kind
        self.size = size
        self.activity = activity
    }

    private var shape: RoundedRectangle {
        RoundedRectangle(cornerRadius: size * 0.3, style: .continuous)
    }

    private var asleep: Bool {
        activity == .asleep
    }

    var body: some View {
        ZStack {
            shape.fill(LinearGradient(
                colors: asleep
                    ? [theme.colors.raised.mix(with: kind.tint, by: 0.14), theme.colors.raised.mix(with: kind.tint, by: 0.3)]
                    : [kind.tint.mix(with: .white, by: 0.2), kind.tint, kind.tint.mix(with: .black, by: 0.2)],
                startPoint: .topLeading, endPoint: .bottomTrailing
            ))
            shape.strokeBorder(
                LinearGradient(colors: [.white.opacity(asleep ? 0.12 : 0.4), .clear, .black.opacity(0.18)],
                               startPoint: .topLeading, endPoint: .bottomTrailing),
                lineWidth: max(0.5, size * 0.03)
            )
            glyph.foregroundStyle(asleep ? kind.tint.mix(with: theme.colors.ink, by: 0.35) : .white)
        }
        .frame(width: size, height: size)
        .shadow(color: theme.colors.shadow.opacity(asleep ? 0 : (theme.isDark ? 0.45 : 0.2)), radius: size * 0.1, x: size * 0.03, y: size * 0.07)
        .overlay {
            if activity == .working {
                WorkingRim(shape: shape, lineWidth: max(1.5, size * 0.06), glow: size * 0.16)
                    .padding(-max(1.5, size * 0.06))
                    .transition(.opacity)
            }
        }
        .animation(theme.motion.fade, value: activity)
        .accessibilityHidden(true)
    }

    @ViewBuilder
    private var glyph: some View {
        if let asset = kind.asset {
            Image(asset).renderingMode(.template).resizable().scaledToFit().frame(width: size * 0.6, height: size * 0.6)
        } else {
            HStack(spacing: size * 0.05) {
                Image(systemName: "chevron.right").font(.system(size: size * 0.38, weight: .heavy))
                RoundedRectangle(cornerRadius: size * 0.03)
                    .fill(asleep ? theme.colors.accent.opacity(0.6) : theme.colors.accent.mix(with: .white, by: 0.25))
                    .frame(width: size * 0.15, height: size * 0.36)
            }
        }
    }
}

struct IconTile: View {
    @Environment(\.theme) private var theme
    let symbol: String
    var tint: Color?
    var size: CGFloat = 28

    var body: some View {
        let color = tint ?? theme.colors.accent
        let shape = RoundedRectangle(cornerRadius: size * 0.3, style: .continuous)
        Image(systemName: symbol)
            .font(.system(size: size * 0.46, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: size, height: size)
            .background {
                shape.fill(LinearGradient(
                    colors: [color.mix(with: .white, by: 0.22), color, color.mix(with: .black, by: 0.2)],
                    startPoint: .topLeading, endPoint: .bottomTrailing
                ))
            }
            .overlay {
                shape.strokeBorder(
                    LinearGradient(colors: [.white.opacity(0.4), .clear, .black.opacity(0.18)], startPoint: .topLeading, endPoint: .bottomTrailing),
                    lineWidth: 0.75
                )
            }
            .shadow(color: theme.colors.shadow.opacity(theme.isDark ? 0.4 : 0.18), radius: size * 0.08, x: 1, y: 2)
            .accessibilityHidden(true)
    }
}

struct RoomSigil: View {
    @Environment(\.theme) private var theme
    let name: String
    let key: String
    var size: CGFloat = 28

    var body: some View {
        let tint = RoomTint.color(for: key)
        let seed = StableHash.value(key)
        let shape = RoundedRectangle(cornerRadius: size * 0.3, style: .continuous)
        let gap = size * 0.07
        let inset = size * 0.2
        let cell = (size - inset * 2 - gap) / 2
        ZStack {
            shape.fill(LinearGradient(
                colors: [tint.mix(with: .white, by: 0.16), tint.mix(with: .black, by: 0.08), tint.mix(with: .black, by: 0.34)],
                startPoint: .topLeading, endPoint: .bottomTrailing
            ))
            VStack(spacing: gap) {
                ForEach(0 ..< 2, id: \.self) { row in
                    HStack(spacing: gap) {
                        ForEach(0 ..< 2, id: \.self) { column in
                            let lit = (seed >> UInt64(row * 2 + column + 3)) & 1 == 1 || row * 2 + column == Int(seed % 4)
                            RoundedRectangle(cornerRadius: cell * 0.28, style: .continuous)
                                .fill(.white.opacity(lit ? 0.92 : 0.28))
                                .frame(width: cell, height: cell)
                        }
                    }
                }
            }
            shape.strokeBorder(
                LinearGradient(colors: [.white.opacity(0.42), .clear, .black.opacity(0.2)], startPoint: .topLeading, endPoint: .bottomTrailing),
                lineWidth: max(0.5, size * 0.03)
            )
        }
        .frame(width: size, height: size)
        .shadow(color: theme.colors.shadow.opacity(theme.isDark ? 0.45 : 0.2), radius: size * 0.1, x: size * 0.03, y: size * 0.07)
        .accessibilityHidden(true)
    }
}

struct DeviceGlyph: View {
    let label: String

    static func symbol(for label: String) -> String {
        let lower = label.lowercased()
        if lower.contains("book") || lower.contains("laptop") || lower.contains("lenovo") || lower.contains("thinkpad") {
            return "laptopcomputer"
        }
        if lower.contains("studio") || lower.contains("mini") || lower.contains("server") || lower.contains("station") {
            return "macstudio"
        }
        if lower.contains("imac") || lower.contains("desktop") {
            return "desktopcomputer"
        }
        return "laptopcomputer"
    }

    var body: some View {
        Image(systemName: Self.symbol(for: label)).accessibilityHidden(true)
    }
}

struct KodosiWordmark: View {
    @Environment(\.theme) private var theme
    var size: CGFloat = 17
    var showsMark = true
    var blinks = true

    var body: some View {
        HStack(spacing: size * 0.5) {
            if showsMark {
                Image(theme.isDark ? "KodosiLogoDark" : "KodosiLogo")
                    .resizable().scaledToFit().frame(width: size * 1.5, height: size * 1.5)
            }
            HStack(alignment: .center, spacing: size * 0.12) {
                Text(verbatim: "kodosi")
                    .font(.system(size: size, weight: .heavy, design: .monospaced))
                    .tracking(-size * 0.02)
                    .foregroundStyle(theme.colors.ink)
                CursorBlock(width: size * 0.5, height: size * 0.92, blinks: blinks)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(verbatim: "Kodosi"))
    }
}
