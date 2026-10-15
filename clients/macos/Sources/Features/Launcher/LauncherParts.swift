import KodosiBridge
import KodosiDesign
import SwiftUI

enum TerminalInk {
    static let body = hex(0x0D0B09)
    static let band = hex(0x1A1512)
    static let key = hex(0x2A221E)
    static let keyLit = hex(0x352B26)
    static let path = hex(0x5DAFA6)
    static let ink = hex(0xF1E9E3)
    static let muted = hex(0xB3A094)
}

struct ChipFace: View {
    var lit = false

    var body: some View {
        BezelBackground(
            shape: RoundedRectangle(cornerRadius: Radius.xs, style: .continuous),
            fill: lit ? TerminalInk.keyLit : TerminalInk.key, bezel: .quiet
        )
    }
}

struct LauncherKey: View {
    var text: String?
    var symbol: String?
    var tint: Color?
    @State private var hovered = false

    var body: some View {
        HStack(spacing: 5) {
            if let symbol {
                Image(systemName: symbol)
                    .font(.system(size: text == nil ? 11 : 10, weight: text == nil ? .heavy : .semibold))
                    .foregroundStyle(text == nil ? TerminalInk.muted : tint ?? TerminalInk.muted)
            }
            if let text {
                Text(verbatim: text)
                    .lineLimit(1).truncationMode(.middle)
                    .foregroundStyle(tint.map { $0.mix(with: .white, by: 0.35) } ?? TerminalInk.ink)
                Image(systemName: "chevron.down")
                    .font(.system(size: 7, weight: .heavy))
                    .foregroundStyle(TerminalInk.muted)
            }
        }
        .padding(.horizontal, text == nil ? 0 : 8).frame(minWidth: 30).frame(height: 26)
        .background { ChipFace(lit: hovered) }
        .contentShape(Rectangle())
        .onHover { hovered = $0 }
    }
}

extension View {
    func launcherMenu(help: LocalizedStringKey, identifier: String) -> some View {
        menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
            .help(help)
            .accessibilityLabel(Text(help))
            .accessibilityIdentifier(identifier)
    }
}

struct AgentTabs: View {
    struct Item: Identifiable, Equatable {
        let id: String
        let kind: AgentKind
        let installed: Bool

        var title: String {
            kind == .shell ? String(localized: "Terminal") : kind.label
        }
    }

    @Environment(\.theme) private var theme
    @Namespace private var tab
    let items: [Item]
    let selection: String
    let hidden: Int
    @Binding var showsAll: Bool
    let select: (String) -> Void

    var body: some View {
        HStack(alignment: .bottom, spacing: 2) {
            ForEach(items) { item in
                mark(item)
            }
            if hidden > 0 {
                Button {
                    withAnimation(theme.motion.spring) { showsAll.toggle() }
                } label: {
                    Image(systemName: "plus")
                        .font(.system(size: 11, weight: .bold))
                        .rotationEffect(.degrees(showsAll ? 45 : 0))
                        .foregroundStyle(TerminalInk.muted)
                        .frame(width: 30, height: 30)
                        .hoverRow(radius: Radius.sm)
                        .padding(.bottom, 2)
                }
                .buttonStyle(PressScaleStyle())
                .help(showsAll ? String(localized: "Hide agents that are not on this Mac")
                    : String(localized: "Show \(hidden) more agents"))
                .accessibilityLabel(Text("More agents"))
                .accessibilityIdentifier("launcher.more")
            }
        }
        .animation(theme.motion.spring, value: items.map(\.id))
    }

    private func mark(_ item: Item) -> some View {
        let selected = item.id == selection
        return Button {
            withAnimation(theme.motion.snappy) { select(item.id) }
        } label: {
            HStack(spacing: 7) {
                AgentMark(kind: item.kind, size: 18, activity: item.installed ? .awake : .asleep)
                if selected {
                    Text(item.title).appTextStyle(.subhead).fixedSize()
                        .foregroundStyle(item.installed ? TerminalInk.ink : TerminalInk.muted)
                        .transition(.opacity)
                }
            }
            .padding(.horizontal, selected ? 12 : 8)
            .frame(height: selected ? 34 : 30)
            .background {
                if selected {
                    TabShape(radius: 9, flare: 8)
                        .fill(TerminalInk.body)
                        .matchedGeometryEffect(id: "tab", in: tab)
                }
            }
            .hoverRow(radius: Radius.sm, selected: selected)
            .padding(.bottom, selected ? 0 : 2)
        }
        .buttonStyle(PressScaleStyle(scale: 0.96))
        .help(item.installed ? item.title : String(localized: "\(item.title) is not on this Mac"))
        .accessibilityLabel(Text(item.title))
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("launcher.agent.\(item.id)")
    }
}

struct TabShape: Shape {
    let radius: CGFloat
    let flare: CGFloat

    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.minX - flare, y: rect.maxY))
        path.addQuadCurve(to: CGPoint(x: rect.minX, y: rect.maxY - flare), control: CGPoint(x: rect.minX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.minY + radius))
        path.addQuadCurve(to: CGPoint(x: rect.minX + radius, y: rect.minY), control: CGPoint(x: rect.minX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX - radius, y: rect.minY))
        path.addQuadCurve(to: CGPoint(x: rect.maxX, y: rect.minY + radius), control: CGPoint(x: rect.maxX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - flare))
        path.addQuadCurve(to: CGPoint(x: rect.maxX + flare, y: rect.maxY), control: CGPoint(x: rect.maxX, y: rect.maxY))
        path.closeSubpath()
        return path
    }
}

@MainActor
func recentFolders(_ deps: AppDependencies, chosen: String?) -> [String] {
    var seen = Set<String>()
    let local: [String?] = deps.sessions.filter { $0.kind == .local }.map(\.localDirectory)
    let candidates: [String?] = [chosen, deps.settings.lastWorkingDir] + local + [deps.storageBootstrap.defaultWorkingDirectory?.path]
    return Array(candidates.compactMap(\.self).filter { seen.insert($0).inserted }.prefix(6))
}

func folderName(_ path: String) -> String {
    tilde(path) == "~" ? "~" : HostPath.folderName(path)
}

func tilde(_ path: String) -> String {
    let home = ProcessInfo.processInfo.environment["HOME"] ?? FileManager.default.homeDirectoryForCurrentUser.path
    return path == home ? "~" : path.hasPrefix(home + "/") ? "~" + path.dropFirst(home.count) : path
}
