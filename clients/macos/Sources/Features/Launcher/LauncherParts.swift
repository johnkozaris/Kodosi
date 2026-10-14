import KodosiDesign
import SwiftUI

/// Text that a terminal types: it erases back to what stays and types the rest.
struct TypedText: View {
    @Environment(\.theme) private var theme
    let text: String
    @State private var shown = ""
    @State private var typing: Task<Void, Never>?

    var body: some View {
        Text(verbatim: shown)
            .onChange(of: text, initial: true) { _, target in type(target) }
            .onDisappear { typing?.cancel() }
            .accessibilityLabel(Text(verbatim: text))
    }

    private func type(_ target: String) {
        typing?.cancel()
        guard !theme.motion.reduced else {
            shown = target
            return
        }
        typing = Task { @MainActor in
            let kept = shown.commonPrefix(with: target).count
            while shown.count > kept {
                shown.removeLast()
                try? await Task.sleep(for: .milliseconds(8))
                guard !Task.isCancelled else { return }
            }
            let rest = target.dropFirst(kept)
            let pace = max(7, min(22, 420 / max(rest.count, 1)))
            for character in rest {
                shown.append(character)
                try? await Task.sleep(for: .milliseconds(pace))
                guard !Task.isCancelled else { return }
            }
        }
    }
}

/// The narrowest width at which a child of a `FlowLayout` takes the rest of
/// its row; with less room it starts a row of its own.
struct FillsRow: LayoutValueKey {
    static let defaultValue: CGFloat? = nil
}

/// Lays its children out in rows, as words wrap on a line.
struct FlowLayout: Layout {
    var spacing: CGFloat = 6
    var lineSpacing: CGFloat = 6

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache _: inout ()) -> CGSize {
        let rows = rows(width: proposal.width ?? .infinity, subviews: subviews)
        let width = rows.map(\.width).max() ?? 0
        let height = rows.map(\.height).reduce(0, +) + lineSpacing * CGFloat(max(rows.count - 1, 0))
        return CGSize(width: width, height: height)
    }

    func placeSubviews(in bounds: CGRect, proposal _: ProposedViewSize, subviews: Subviews, cache _: inout ()) {
        var y = bounds.minY
        for row in rows(width: bounds.width, subviews: subviews) {
            var x = bounds.minX
            for (index, size) in row.items {
                // Words share the middle of the row's first line; a filling child starts at its top.
                let top = subviews[index][FillsRow.self] == nil ? (row.line - size.height) / 2 : 0
                subviews[index].place(at: CGPoint(x: x, y: y + top), anchor: .topLeading, proposal: ProposedViewSize(size))
                x += size.width + spacing
            }
            y += row.height + lineSpacing
        }
    }

    private struct Row {
        var items: [(index: Int, size: CGSize)] = []
        var width: CGFloat = 0
        var height: CGFloat = 0
        /// The height of the row's words, without a filling child.
        var line: CGFloat = 0
    }

    private func rows(width: CGFloat, subviews: Subviews) -> [Row] {
        var rows: [Row] = []
        var row = Row()
        for index in subviews.indices {
            let subview = subviews[index]
            let used = row.items.isEmpty ? 0 : row.width + spacing
            var size: CGSize
            if let minimum = subview[FillsRow.self], width.isFinite {
                if width - used < minimum, !row.items.isEmpty {
                    rows.append(row)
                    row = Row()
                }
                let start = row.items.isEmpty ? 0 : row.width + spacing
                size = subview.sizeThatFits(ProposedViewSize(width: width - start, height: nil))
            } else {
                size = subview.sizeThatFits(.unspecified)
                if size.width > width {
                    size = subview.sizeThatFits(ProposedViewSize(width: width, height: nil))
                }
                if used + size.width > width, !row.items.isEmpty {
                    rows.append(row)
                    row = Row()
                }
            }
            row.width = row.items.isEmpty ? size.width : row.width + spacing + size.width
            row.height = max(row.height, size.height)
            if subview[FillsRow.self] == nil {
                row.line = max(row.line, size.height)
            }
            row.items.append((index, size))
        }
        if !row.items.isEmpty {
            rows.append(row)
        }
        return rows
    }
}

/// The terminal's own colors, which stay dark in both looks: its body, its
/// band and the face of a key.
enum TerminalInk {
    static let body = hex(0x0D0B09)
    static let band = hex(0x1A1512)
    static let key = hex(0x2A221E)
    static let keyLit = hex(0x352B26)
    static let path = hex(0x5DAFA6)
}

/// The face of a key on the command line: one solid color, lit from the top
/// left as the buttons are.
struct ChipFace: View {
    var lit = false

    var body: some View {
        BezelBackground(
            shape: RoundedRectangle(cornerRadius: Radius.xs, style: .continuous),
            fill: lit ? TerminalInk.keyLit : TerminalInk.key, bezel: .quiet
        )
    }
}

/// One part of the command line, as a key that opens its list in the terminal.
/// A part not chosen yet names what it holds in muted words.
struct CommandToken: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let text: String
    var symbol: String?
    var badge: String?
    var slot = false
    var tint: Color?
    var loading = false
    var open = false
    let identifier: String
    let help: LocalizedStringKey
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 5) {
                if let symbol {
                    Image(systemName: symbol)
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(tint ?? theme.colors.inkMuted)
                        .symbolEffect(.bounce, value: text)
                }
                TypedText(text: text)
                    .lineLimit(1).truncationMode(.middle)
                    .foregroundStyle(foreground)
                    .shimmer(loading)
                if let badge {
                    Text(verbatim: "· \(badge)")
                        .foregroundStyle(theme.colors.inkMuted)
                        .lineLimit(1)
                        .transition(.opacity.combined(with: .offset(x: -4)))
                }
                Image(systemName: "chevron.down")
                    .font(.system(size: 7, weight: .heavy))
                    .foregroundStyle(open ? theme.colors.accent : theme.colors.inkMuted)
                    .rotationEffect(.degrees(open ? 180 : 0))
            }
            .padding(.horizontal, 8).frame(height: 26)
            .background { ChipFace(lit: hovered || open) }
            .offset(y: hovered && !open ? -0.5 : 0)
            .contentShape(Rectangle())
        }
        .buttonStyle(PressScaleStyle(scale: 0.95))
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .animation(theme.motion.snappy, value: open)
        .help(help)
        .accessibilityAddTraits(open ? .isSelected : [])
        .accessibilityLabel(Text(help))
        .accessibilityValue(Text(verbatim: [text, badge].compactMap(\.self).joined(separator: " ")))
        .accessibilityIdentifier(identifier)
    }

    private var foreground: Color {
        if slot {
            return theme.colors.inkMuted
        }
        return tint.map { $0.mix(with: .white, by: 0.35) } ?? theme.colors.ink
    }
}

/// The dots that show the keys for the model and the effort.
struct MoreKey: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: "ellipsis")
                .font(.system(size: 11, weight: .heavy))
                .foregroundStyle(theme.colors.inkMuted)
                .frame(width: 30, height: 26)
                .background { ChipFace(lit: hovered) }
                .offset(y: hovered ? -0.5 : 0)
                .contentShape(Rectangle())
        }
        .buttonStyle(PressScaleStyle(scale: 0.95))
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .help("Model and effort")
        .accessibilityIdentifier("launcher.moreChoices")
    }
}

/// The cursor at the end of the command line. It stays lit while the line
/// changes, then blinks as a terminal cursor does.
struct PromptCursor: View {
    @Environment(\.theme) private var theme
    let line: String
    @State private var lit = true

    var body: some View {
        CursorBlock(width: 9, height: 18, blinks: false)
            .opacity(lit ? 1 : 0)
            .task(id: line) {
                lit = true
                guard !theme.motion.reduced else { return }
                try? await Task.sleep(for: .milliseconds(1100))
                while !Task.isCancelled {
                    withAnimation(.easeInOut(duration: 0.08)) { lit.toggle() }
                    try? await Task.sleep(for: .milliseconds(lit ? 560 : 440))
                }
            }
    }
}

/// The band of the launcher's terminal: a tab for each kind of terminal it
/// opens, the plain terminal first. The chosen tab opens into the terminal
/// below it and says its name; the others are marks. Agents that are not here
/// wait behind the plus, and an agent that arrives says so once.
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
    let fresh: Set<String>
    @Binding var showsAll: Bool
    let select: (String) -> Void

    var body: some View {
        HStack(alignment: .bottom, spacing: 2) {
            ForEach(items) { item in
                mark(item).transition(AnyTransition.pop)
            }
            if hidden > 0 {
                Button {
                    withAnimation(theme.motion.spring) { showsAll.toggle() }
                } label: {
                    Image(systemName: "plus")
                        .font(.system(size: 11, weight: .bold))
                        .rotationEffect(.degrees(showsAll ? 45 : 0))
                        .foregroundStyle(theme.colors.inkMuted)
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
        .animation(theme.motion.spring, value: fresh)
    }

    private func mark(_ item: Item) -> some View {
        let selected = item.id == selection
        let arrived = fresh.contains(item.id) && !selected
        return Button {
            withAnimation(theme.motion.snappy) { select(item.id) }
        } label: {
            HStack(spacing: 7) {
                AgentMark(kind: item.kind, size: 18, activity: item.installed ? .awake : .asleep)
                    .keyframeAnimator(initialValue: 1.0, trigger: selected) { mark, scale in
                        mark.scaleEffect(selected && !theme.motion.reduced ? scale : 1)
                    } keyframes: { _ in
                        SpringKeyframe(0.8, duration: 0.09, spring: .snappy)
                        SpringKeyframe(1.0, duration: 0.45, spring: .bouncy)
                    }
                if selected || arrived {
                    Text(item.title).appTextStyle(.subhead).fixedSize()
                        .foregroundStyle(arrived ? theme.colors.accentStrong : item.installed ? theme.colors.ink : theme.colors.inkMuted)
                        .transition(.opacity.combined(with: .offset(x: -6)))
                }
            }
            .padding(.horizontal, selected || arrived ? 12 : 8)
            .frame(height: selected ? 34 : 30)
            .background {
                if selected {
                    TabShape(radius: 9, flare: 8)
                        .fill(TerminalInk.body)
                        .matchedGeometryEffect(id: "tab", in: tab)
                } else if arrived {
                    Ripple(color: theme.colors.accent)
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

/// A tab that opens into what is below it: round on top, and its bottom
/// corners flare out into the edge it stands on.
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

/// A ring that leaves its place once, for something that just arrived.
struct Ripple: View {
    @Environment(\.theme) private var theme
    let color: Color
    @State private var out = false

    var body: some View {
        RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
            .strokeBorder(color, lineWidth: 1.5)
            .scaleEffect(out ? 1.25 : 1)
            .opacity(out ? 0 : 1)
            .onAppear {
                guard !theme.motion.reduced else { return }
                withAnimation(.easeOut(duration: 1.1).repeatCount(2, autoreverses: false)) { out = true }
            }
    }
}

/// How the launcher's terminal first appears: the copper cursor alone, then a
/// line that grows from it, then the window around the line.
struct SeedMask: Shape {
    /// From 0, the cursor, to 1, the whole window.
    var progress: CGFloat
    let cursor: CGRect
    let radius: CGFloat

    var animatableData: CGFloat {
        get { progress }
        set { progress = newValue }
    }

    func path(in rect: CGRect) -> Path {
        let wide = min(1, max(0, progress / 0.45))
        let tall = min(1, max(0, (progress - 0.45) / 0.55))
        let line = CGRect(
            x: cursor.minX + (rect.minX - cursor.minX) * wide, y: cursor.minY,
            width: cursor.width + (rect.width - cursor.width) * wide, height: cursor.height
        )
        let frame = CGRect(
            x: line.minX, y: line.minY + (rect.minY - line.minY) * tall,
            width: line.width, height: line.height + (rect.height - line.height) * tall
        )
        return Path(roundedRect: frame, cornerRadius: 2 + (radius - 2) * tall, style: .continuous)
    }
}
