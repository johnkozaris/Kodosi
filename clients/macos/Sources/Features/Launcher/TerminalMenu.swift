import AppKit
import KodosiDesign
import SwiftUI

struct TerminalMenu: View {
    enum LineKind: Equatable {
        case comment, choice, action
    }

    struct Line: Identifiable, Equatable {
        let id: String
        var kind = LineKind.choice
        let title: String
        var detail: String?
        var current = false
    }

    @Environment(\.theme) private var theme
    let lines: [Line]
    let room: CGFloat
    let identifier: String
    let pick: (String) -> Void
    let close: () -> Void
    @State private var highlighted: String?
    @State private var filter = ""
    @State private var shown = 0
    @FocusState private var focused: Bool

    static let rowHeight: CGFloat = 24
    private static let visibleRows = 9
    private static let font = NSFont.monospacedSystemFont(ofSize: 13, weight: .medium)
    private static let cell = ("M" as NSString).size(withAttributes: [.font: font]).width

    private var matches: [Line] {
        let query = filter.trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else { return lines }
        var result: [Line] = []
        var comment: Line?
        for line in lines {
            guard line.kind != .comment else {
                comment = line
                continue
            }
            guard line.title.localizedCaseInsensitiveContains(query)
                || line.detail?.localizedCaseInsensitiveContains(query) == true
            else { continue }
            if let pending = comment {
                result.append(pending)
                comment = nil
            }
            result.append(line)
        }
        return result
    }

    var body: some View {
        let matches = matches
        let column = matches.filter { $0.kind != .comment }.map(\.title.count).max() ?? 0
        VStack(alignment: .leading, spacing: 0) {
            if !filter.isEmpty {
                HStack(spacing: 0) {
                    Text(verbatim: "❯ ").foregroundStyle(theme.colors.accent)
                    Text(verbatim: filter).foregroundStyle(theme.colors.ink)
                    CursorBlock(width: 8, height: 15, blinks: false)
                }
                .frame(height: Self.rowHeight).padding(.leading, 20)
                .transition(.opacity)
            }
            ScrollViewReader { proxy in
                ScrollView(.vertical) {
                    VStack(alignment: .leading, spacing: 0) {
                        ForEach(Array(matches.enumerated()), id: \.element.id) { index, line in
                            row(line, column: column)
                                .opacity(index < shown ? 1 : 0)
                                .offset(y: index < shown ? 0 : -4)
                                .id(line.id)
                        }
                        if matches.isEmpty {
                            Text("# nothing matches")
                                .foregroundStyle(theme.colors.inkFaint)
                                .frame(height: Self.rowHeight).padding(.leading, 20)
                        }
                    }
                }
                .scrollIndicators(matches.count > Self.visibleRows ? .automatic : .never)
                .scrollDisabled(matches.count <= Self.visibleRows)
                .frame(height: CGFloat(min(max(matches.count, 1), Self.visibleRows)) * Self.rowHeight)
                .onChange(of: highlighted) { _, id in
                    if let id {
                        proxy.scrollTo(id)
                    }
                }
            }
        }
        .font(.system(size: 13, weight: .medium, design: .monospaced))
        .frame(width: min(room, width(matches, column: column)), alignment: .leading)
        .focusable()
        .focused($focused)
        .focusEffectDisabled()
        .onKeyPress(.upArrow) { move(-1) }
        .onKeyPress(.downArrow) { move(1) }
        .onKeyPress(.return) {
            if let highlighted, matches.contains(where: { $0.id == highlighted }) {
                pick(highlighted)
            }
            return .handled
        }
        .onKeyPress(.escape) {
            if filter.isEmpty {
                close()
            } else {
                filter = ""
            }
            return .handled
        }
        .onKeyPress(.delete) {
            guard !filter.isEmpty else { return .ignored }
            filter.removeLast()
            return .handled
        }
        .onKeyPress(phases: .down) { press in
            guard press.modifiers.isDisjoint(with: [.command, .control, .option]),
                  let character = press.characters.first, !character.isNewline, character.isLetter || character.isNumber
                  || character.isPunctuation || character.isSymbol || (character == " " && !filter.isEmpty)
            else { return .ignored }
            filter.append(press.characters)
            return .handled
        }
        .onChange(of: filter) { _, _ in
            highlighted = self.matches.first { $0.kind != .comment }?.id
        }
        .onChange(of: focused) { _, focused in
            if !focused {
                close()
            }
        }
        .task(id: identifier) { await reveal() }
        .onAppear {
            focused = true
            highlighted = (lines.first(where: \.current) ?? lines.first { $0.kind != .comment })?.id
        }
        .accessibilityElement(children: .contain)
        .accessibilityIdentifier(identifier)
    }

    private func row(_ line: Line, column: Int) -> some View {
        let lit = line.id == highlighted && line.kind != .comment
        return HStack(spacing: 0) {
            RoundedRectangle(cornerRadius: 1.5, style: .continuous)
                .fill(lit ? TerminalInk.body : theme.colors.accent)
                .frame(width: 6, height: 15)
                .opacity(line.current ? 1 : 0)
                .frame(width: 20)
            if line.kind == .comment {
                Text(verbatim: "# \(line.title)").foregroundStyle(theme.colors.inkMuted).fontWeight(.regular)
                if let detail = line.detail {
                    Text(verbatim: "  \(detail)").foregroundStyle(theme.colors.inkFaint).fontWeight(.regular)
                }
            } else {
                Text(verbatim: line.title.padding(toLength: max(column, line.title.count), withPad: " ", startingAt: 0))
                    .foregroundStyle(lit ? TerminalInk.body : line.kind == .action ? theme.colors.accentStrong : theme.colors.ink)
                if let detail = line.detail {
                    Text(verbatim: "  \(detail)")
                        .foregroundStyle(lit ? TerminalInk.body.opacity(0.75) : theme.colors.inkFaint)
                        .fontWeight(.regular)
                }
            }
            Spacer(minLength: 8)
        }
        .lineLimit(1).truncationMode(.middle)
        .frame(height: Self.rowHeight)
        .padding(.trailing, 4)
        .background {
            if lit {
                RoundedRectangle(cornerRadius: 3, style: .continuous).fill(theme.colors.accent)
            }
        }
        .contentShape(Rectangle())
        .onHover { inside in
            if inside, line.kind != .comment {
                highlighted = line.id
            }
        }
        .onTapGesture {
            if line.kind != .comment {
                pick(line.id)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(verbatim: [line.title, line.detail].compactMap(\.self).joined(separator: ", ")))
        .accessibilityAddTraits(line.kind == .comment ? .isHeader : line.current ? [.isButton, .isSelected] : .isButton)
        .accessibilityAction {
            if line.kind != .comment {
                pick(line.id)
            }
        }
        .accessibilityIdentifier("\(identifier).\(AccessibilityIdentifier.token(line.id))")
    }

    private func width(_ lines: [Line], column: Int) -> CGFloat {
        let cells = lines.map { line in
            let detail = line.detail.map { $0.count + 2 } ?? 0
            return line.kind == .comment ? line.title.count + 2 + detail : max(column, line.title.count) + detail
        }
        let bar: CGFloat = lines.count > Self.visibleRows ? 16 : 0
        return 20 + CGFloat(max(cells.max() ?? 0, filter.count + 2)) * Self.cell + 16 + bar
    }

    private func move(_ step: Int) -> KeyPress.Result {
        let choices = matches.filter { $0.kind != .comment }
        guard !choices.isEmpty else { return .handled }
        let index = choices.firstIndex { $0.id == highlighted } ?? (step > 0 ? -1 : 0)
        highlighted = choices[(index + step + choices.count) % choices.count].id
        return .handled
    }

    private func reveal() async {
        guard !theme.motion.reduced else {
            shown = .max
            return
        }
        shown = 0
        for index in 1 ... max(lines.count, 1) {
            withAnimation(.easeOut(duration: 0.16)) { shown = index }
            try? await Task.sleep(for: .milliseconds(14))
            guard !Task.isCancelled else { return }
        }
        shown = .max
    }
}

struct NewAccountLine: View {
    @Environment(\.theme) private var theme
    let folder: String
    let error: String?
    let saving: Bool
    let add: (String) -> Void
    let cancel: () -> Void
    @State private var name = ""
    @FocusState private var focused: Bool

    private var slug: String {
        name.trimmingCharacters(in: .whitespaces).lowercased().split(separator: " ").joined(separator: "-")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 0) {
                Text(verbatim: "+ ").foregroundStyle(theme.colors.accentStrong)
                    .frame(width: 20, alignment: .trailing)
                TextField("name", text: $name, prompt: Text("name").foregroundStyle(theme.colors.inkFaint))
                    .textFieldStyle(.plain)
                    .foregroundStyle(theme.colors.ink)
                    .focused($focused)
                    .onSubmit {
                        if !slug.isEmpty, !saving {
                            add(name)
                        }
                    }
                    .onExitCommand(perform: cancel)
                    .frame(width: 180)
                    .shimmer(saving)
                    .accessibilityIdentifier("launcher.account.name")
            }
            .frame(height: TerminalMenu.rowHeight)
            Text(verbatim: "# \(slug.isEmpty ? String(localized: "a sign-in of its own") : "\(folder)-\(slug)")")
                .foregroundStyle(theme.colors.inkFaint).fontWeight(.regular)
                .frame(height: TerminalMenu.rowHeight).padding(.leading, 20)
                .contentTransition(.numericText())
            if let error {
                Text(verbatim: "# \(error)")
                    .foregroundStyle(theme.colors.caution).fontWeight(.regular)
                    .lineLimit(2)
                    .padding(.leading, 20)
                    .transition(.opacity)
            }
        }
        .font(.system(size: 13, weight: .medium, design: .monospaced))
        .animation(theme.motion.snappy, value: slug)
        .onAppear { focused = true }
    }
}
