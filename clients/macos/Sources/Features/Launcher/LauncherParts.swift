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

/// The face of a choice on the command line: a small key lit from the top
/// left, as the buttons are, in the agent's color when it has one.
struct ChipFace: View {
    @Environment(\.theme) private var theme
    var tint: Color?
    var lit = false

    var body: some View {
        let colors = theme.colors
        let face = colors.lifted.mix(with: colors.ink, by: (theme.isDark ? 0.07 : 0) + (lit ? 0.04 : 0))
        BezelBackground(
            shape: RoundedRectangle(cornerRadius: Radius.xs, style: .continuous),
            fill: tint.map { face.mix(with: $0, by: theme.isDark ? 0.22 : 0.12) } ?? face,
            bezel: .quiet
        )
    }
}

/// One part of the command line, as a key that opens its choices. A part not
/// chosen yet names what it holds in faint words.
struct CommandToken<Choices: View>: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    @State private var presented = false
    let text: String
    var symbol: String?
    var badge: String?
    var slot = false
    var tint: Color?
    var loading = false
    let identifier: String
    let help: LocalizedStringKey
    @ViewBuilder let choices: (_ close: @escaping () -> Void) -> Choices

    var body: some View {
        Button { presented.toggle() } label: {
            HStack(spacing: 5) {
                if let symbol {
                    Image(systemName: symbol)
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(foreground)
                        .symbolEffect(.bounce, value: text)
                }
                TypedText(text: text)
                    .lineLimit(1).truncationMode(.middle)
                    .foregroundStyle(foreground)
                    .shimmer(loading)
                if let badge {
                    Text(verbatim: badge)
                        .font(.system(size: 11, weight: .semibold, design: .rounded))
                        .foregroundStyle(foreground)
                        .padding(.horizontal, 5).frame(height: 17)
                        .background((tint ?? theme.colors.ink).opacity(0.2), in: RoundedRectangle(cornerRadius: 4, style: .continuous))
                        .transition(AnyTransition.pop)
                }
                Image(systemName: "chevron.down")
                    .font(.system(size: 7, weight: .heavy))
                    .foregroundStyle(slot ? theme.colors.inkFaint : theme.colors.inkMuted)
                    .rotationEffect(.degrees(presented ? 180 : 0))
            }
            .padding(.horizontal, 8).frame(height: 26)
            .background { ChipFace(tint: slot ? nil : tint, lit: hovered || presented) }
            .offset(y: hovered && !presented ? -0.5 : 0)
            .contentShape(Rectangle())
        }
        .buttonStyle(PressScaleStyle(scale: 0.95))
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .animation(theme.motion.snappy, value: presented)
        .popover(isPresented: $presented, arrowEdge: .bottom) {
            choices { presented = false }.chromeScope()
        }
        .help(help)
        .accessibilityLabel(Text(help))
        .accessibilityValue(Text(verbatim: [text, badge].compactMap(\.self).joined(separator: " ")))
        .accessibilityIdentifier(identifier)
    }

    private var foreground: Color {
        if slot {
            return theme.colors.inkMuted
        }
        return tint.map { $0.mix(with: theme.colors.ink, by: theme.isDark ? 0.45 : 0.2) } ?? theme.colors.ink
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

struct ChoiceRow: View {
    @Environment(\.theme) private var theme
    let title: String
    var detail: String?
    var monoDetail = false
    var selected = false
    var symbol: String?
    let identifier: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 10) {
                Image(systemName: symbol ?? "checkmark")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(theme.colors.accentStrong)
                    .opacity(symbol != nil || selected ? 1 : 0)
                    .frame(width: 14)
                VStack(alignment: .leading, spacing: 1) {
                    Text(verbatim: title).appTextStyle(.body).foregroundStyle(symbol == nil ? theme.colors.ink : theme.colors.accentStrong)
                        .lineLimit(1)
                    if let detail {
                        Text(verbatim: detail).appTextStyle(monoDetail ? .monoCaption : .caption).fontWeight(.regular)
                            .foregroundStyle(theme.colors.inkMuted).lineLimit(1).truncationMode(.middle)
                    }
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8).padding(.vertical, 6)
            .contentShape(Rectangle())
            .hoverRow(radius: Radius.sm)
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier(identifier)
    }
}

struct ChoicePanel<Content: View>: View {
    @Environment(\.theme) private var theme
    let title: LocalizedStringKey
    @ViewBuilder let content: () -> Content

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint).padding(.horizontal, 10)
            content()
        }
        .padding(8)
        .popoverSheet(width: 300)
    }
}

/// The agents of this Mac as a row of marks on a sliding thumb. The plain
/// terminal comes first; agents that are not here show pale behind the plus.
struct AgentDock: View {
    struct Item: Identifiable, Equatable {
        let id: String
        let kind: AgentKind
        let installed: Bool

        var title: String {
            kind == .shell ? String(localized: "Terminal") : kind.label
        }
    }

    @Environment(\.theme) private var theme
    @Namespace private var thumb
    let items: [Item]
    let selection: String
    let hidden: Int
    @Binding var showsAll: Bool
    let select: (String) -> Void

    var body: some View {
        HStack(spacing: 2) {
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
                        .frame(width: 30, height: 32)
                        .hoverRow(radius: Radius.sm)
                }
                .buttonStyle(PressScaleStyle())
                .help(showsAll ? String(localized: "Hide agents that are not on this Mac")
                    : String(localized: "Show \(hidden) more agents"))
                .accessibilityLabel(Text("More agents"))
                .accessibilityIdentifier("launcher.more")
            }
        }
        .padding(3)
        .well(Radius.sm + 3)
        .animation(theme.motion.spring, value: items.map(\.id))
    }

    private func mark(_ item: Item) -> some View {
        let selected = item.id == selection
        let shape = RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
        return Button {
            withAnimation(theme.motion.snappy) { select(item.id) }
        } label: {
            HStack(spacing: 7) {
                AgentMark(kind: item.kind, size: 20, activity: item.installed ? .awake : .asleep)
                    .keyframeAnimator(initialValue: 1.0, trigger: selected) { mark, scale in
                        mark.scaleEffect(selected && !theme.motion.reduced ? scale : 1)
                    } keyframes: { _ in
                        SpringKeyframe(0.84, duration: 0.09, spring: .snappy)
                        SpringKeyframe(1.0, duration: 0.45, spring: .bouncy)
                    }
                if selected {
                    Text(item.title).appTextStyle(.footnote).fontWeight(.semibold).fixedSize()
                        .foregroundStyle(item.installed ? theme.colors.ink : theme.colors.inkMuted)
                        .transition(.opacity.combined(with: .offset(x: -6)))
                }
            }
            .padding(.leading, 6).padding(.trailing, selected ? 11 : 6)
            .frame(height: 32)
            .background {
                if selected {
                    BezelBackground(
                        shape: shape,
                        fill: theme.colors.lifted.mix(with: theme.colors.ink, by: theme.isDark ? 0.07 : 0),
                        bezel: .quiet
                    )
                    .matchedGeometryEffect(id: "thumb", in: thumb)
                }
            }
            .hoverRow(radius: Radius.sm, selected: selected)
        }
        .buttonStyle(PressScaleStyle(scale: 0.96))
        .help(item.installed ? item.title : String(localized: "\(item.title) is not on this Mac"))
        .accessibilityLabel(Text(item.title))
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityIdentifier("launcher.agent.\(item.id)")
    }
}
