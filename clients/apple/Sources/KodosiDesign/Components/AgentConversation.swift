import Foundation
import SwiftUI

extension EnvironmentValues {
    /// The folder of the terminal, from which a path in an agent's prose names a file.
    @Entry var proseFolder: String?
    /// A long press on the message opens its own menu, so its texts do not take the long press.
    @Entry var messageMenu = false
}

extension View {
    /// Selectable text, except in a message whose long press opens the message's own menu.
    func messageTextSelection() -> some View {
        modifier(MessageTextSelection())
    }

    /// Lets a person copy the whole message: on a Mac with a key that shows while the pointer is on
    /// it, on an iPhone or iPad from the menu of a long press, which also selects a part of it. A
    /// message still being written has no copy yet. An answer has the key at its bottom, where a
    /// reader of a long one ends up.
    func copiesMessage(
        _ text: String?, at edge: VerticalEdge = .top,
        preview: some Shape = RoundedRectangle(cornerRadius: Radius.md, style: .continuous),
    ) -> some View {
        modifier(MessageCopy(text: text, edge: edge, preview: AnyShape(preview)))
    }
}

private struct MessageTextSelection: ViewModifier {
    @Environment(\.messageMenu) private var messageMenu

    func body(content: Content) -> some View {
        if messageMenu {
            content
        } else {
            content.textSelection(.enabled)
        }
    }
}

private struct MessageCopy: ViewModifier {
    @Environment(\.theme) private var theme
    let text: String?
    let edge: VerticalEdge
    let preview: AnyShape
    #if os(macOS)
        @State private var hovered = false
    #else
        @State private var selecting = false
    #endif

    func body(content: Content) -> some View {
        #if os(macOS)
            content
                .contentShape(Rectangle())
                .overlay(alignment: edge == .top ? .topTrailing : .bottomTrailing) {
                    if hovered, let text {
                        MessageCopyKey(text: text)
                            .offset(y: edge == .top ? -10 : 10)
                            .transition(.opacity.combined(with: .scale(scale: 0.94, anchor: .trailing)))
                    }
                }
                .onHover { hovered = $0 }
                .animation(theme.motion.hover, value: hovered)
                .accessibilityActions { copyAction }
        #else
            content
                .environment(\.messageMenu, true)
                .contentShape(.contextMenuPreview, preview)
                .contextMenu {
                    if let text {
                        Button { Pasteboard.copy(text) } label: {
                            Label { Text("Copy", bundle: .module) } icon: { Image(systemName: "doc.on.doc") }
                        }
                        Button { selecting = true } label: {
                            Label { Text("Select text", bundle: .module) } icon: { Image(systemName: "selection.pin.in.out") }
                        }
                    }
                }
                .sheet(isPresented: $selecting) { SelectableTextSheet(text: text ?? "") }
                .accessibilityActions { copyAction }
        #endif
    }

    @ContentBuilder
    private var copyAction: some View {
        if let text {
            Button { Pasteboard.copy(text) } label: { Text("Copy", bundle: .module) }
        }
    }
}

#if os(macOS)
    private struct MessageCopyKey: View {
        @Environment(\.theme) private var theme
        let text: String
        @State private var copied = false

        var body: some View {
            IconButton(
                title: copied ? "Copied" : "Copy", bundle: .module, symbol: copied ? "checkmark" : "doc.on.doc",
                identifier: "chat.message.copy", size: 24, active: copied,
            ) {
                Pasteboard.copy(text)
                copied = true
            }
            .padding(2)
            .raisedCapsule(fill: theme.colors.lifted, elevation: .lifted)
        }
    }
#endif

struct PersonBubble: View {
    @Environment(\.theme) private var theme
    let text: String
    var copies = false

    var body: some View {
        HStack(spacing: 0) {
            Spacer(minLength: 56)
            let bubble = Text(ProjectMessageText.formatted(text))
                .appTextStyle(.body).lineSpacing(2).foregroundStyle(theme.colors.ink).messageTextSelection()
                .projectBubble()
            if copies {
                bubble.copiesMessage(text, preview: ProjectBubble())
            } else {
                bubble
            }
        }
    }
}

struct AgentProse: View {
    @Environment(\.theme) private var theme
    @Environment(\.proseFolder) private var folder
    let text: String
    let live: Bool

    var body: some View {
        let blocks = ProseBlock.blocks(of: text)
        VStack(alignment: .leading, spacing: 9) {
            ForEach(blocks.indices, id: \.self) { index in
                let last = index == blocks.count - 1
                switch blocks[index] {
                case let .lines(lines):
                    HStack(alignment: .lastTextBaseline, spacing: 3) {
                        Text(Self.marked(lines, folder: folder, code: theme.colors.ink.opacity(0.08)))
                            .appTextStyle(.body).lineSpacing(3).foregroundStyle(theme.colors.ink).messageTextSelection()
                            .tint(theme.colors.accentStrong)
                        if last, live {
                            CursorBlock(width: 6, height: 14, blink: .typing).transition(.opacity)
                        }
                    }
                case let .heading(heading):
                    Text(ProjectMessageText.formatted(heading))
                        .appTextStyle(.subhead).foregroundStyle(theme.colors.ink).messageTextSelection()
                        .padding(.top, index == 0 ? 0 : 3)
                case let .code(code):
                    ScrollView(.horizontal) {
                        Text(verbatim: code).appTextStyle(.monoCaption).foregroundStyle(theme.colors.ink).messageTextSelection()
                            .fixedSize().padding(.horizontal, 10).padding(.vertical, 8)
                    }
                    .scrollIndicators(.never)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .well(Radius.md)
                case let .table(table):
                    ProseTableView(table: table)
                }
            }
        }
        .animation(theme.motion.fade, value: live)
    }
}

extension AgentProse {
    /// Inline code stands out as it does in the agent's terminal, and a path of a file is a link to
    /// it, from the terminal's folder.
    static func marked(_ text: String, folder: String?, code: Color) -> AttributedString {
        var value = ProjectMessageText.formatted(text)
        for run in value.runs where run.inlinePresentationIntent?.contains(.code) == true {
            value[run.range].backgroundColor = code
        }
        guard let filePath else { return value }
        let plain = String(value.characters)
        for match in filePath.matches(in: plain, range: NSRange(plain.startIndex..., in: plain)) {
            guard let found = Range(match.range, in: plain), let range = Range(match.range, in: value), value[range].link == nil,
                  let file = fileURL(String(plain[found]), folder: folder)
            else { continue }
            value[range].link = file
        }
        return value
    }

    /// A path as Ghostty finds one: names with slashes between them, and a line after a colon. A
    /// path that does not start at a folder ends with the extension of a file, so words such as
    /// "and/or" and numbers such as 3/4 stay text.
    private static let filePath = try? NSRegularExpression(pattern: #"(?<![\w/.:@-])(?:~/|\.{1,2}/|/)?(?:[\w.-]+/)+[\w.-]+(?::\d+){0,2}"#)

    private static func fileURL(_ found: String, folder: String?) -> URL? {
        let path = found.replacing(/(?::\d+){1,2}$/, with: "")
        let rooted = ["/", "~/", "./", "../"].contains { path.hasPrefix($0) }
        guard rooted || path.split(separator: "/").last?.contains(/\.[A-Za-z0-9]*[A-Za-z][A-Za-z0-9]*$/) == true else { return nil }
        if path.hasPrefix("/") || path.hasPrefix("~/") {
            return URL(filePath: NSString(string: path).expandingTildeInPath)
        }
        guard let folder else { return nil }
        return URL(filePath: folder).appending(path: path).standardizedFileURL
    }
}

enum ProseBlock: Equatable {
    case lines(String)
    case heading(String)
    case code(String)
    case table(ProseTable)

    /// Treats an unclosed code fence as a code block through the end of the text while streaming.
    static func blocks(of text: String) -> [Self] {
        var blocks: [Self] = []
        var lines: [Substring] = []
        var code: [Substring]?
        var table: [Substring] = []
        func flush() {
            if !table.isEmpty {
                blocks.append(.table(ProseTable(lines: table)))
                table = []
            }
            let joined = lines.joined(separator: "\n").trimmingCharacters(in: .newlines)
            if !joined.isEmpty {
                blocks.append(.lines(joined))
            }
            lines = []
        }
        for line in text.split(separator: "\n", omittingEmptySubsequences: false) {
            let trimmed = line.drop { $0 == " " }
            if trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~") {
                if let open = code {
                    blocks.append(.code(open.joined(separator: "\n")))
                    code = nil
                } else {
                    flush()
                    code = []
                }
            } else if code != nil {
                code?.append(line)
            } else if trimmed.hasPrefix("|") {
                if table.isEmpty {
                    flush()
                }
                table.append(line)
            } else if let heading = heading(trimmed) {
                flush()
                blocks.append(.heading(heading))
            } else {
                if !table.isEmpty {
                    flush()
                }
                lines.append(bulleted(line))
            }
        }
        if let open = code {
            blocks.append(.code(open.joined(separator: "\n")))
        }
        flush()
        return blocks
    }

    private static func heading(_ line: Substring) -> String? {
        let marks = line.prefix { $0 == "#" }
        guard (1 ... 6).contains(marks.count), line.dropFirst(marks.count).first == " " else { return nil }
        return String(line.dropFirst(marks.count + 1))
    }

    private static func bulleted(_ line: Substring) -> Substring {
        let indent = line.prefix { $0 == " " }
        let rest = line.dropFirst(indent.count)
        guard rest.hasPrefix("- ") || rest.hasPrefix("* ") || rest.hasPrefix("+ ") else { return line }
        return indent + "• " + rest.dropFirst(2)
    }
}

struct ProseTable: Equatable {
    enum Side: Equatable {
        case leading, center, trailing
    }

    private(set) var header: [String] = []
    private(set) var rows: [[String]] = []
    private(set) var sides: [Side] = []

    init(lines: [Substring]) {
        var rows = lines.map(Self.cells)
        if rows.count > 1, let sides = Self.sides(rows[1]) {
            header = rows[0]
            self.sides = sides
            rows.removeFirst(2)
        }
        self.rows = rows
    }

    var columns: Int {
        max(header.count, rows.map(\.count).max() ?? 0)
    }

    func side(of column: Int) -> Side {
        column < sides.count ? sides[column] : .leading
    }

    static func cells(_ line: Substring) -> [String] {
        var cells: [String] = []
        var cell = ""
        var escaped = false
        for character in line {
            if escaped {
                if character != "|" {
                    cell.append("\\")
                }
                cell.append(character)
                escaped = false
            } else if character == "\\" {
                escaped = true
            } else if character == "|" {
                cells.append(cell)
                cell = ""
            } else {
                cell.append(character)
            }
        }
        if escaped {
            cell.append("\\")
        }
        cells.append(cell)
        cells = cells.map { $0.trimmingCharacters(in: .whitespaces) }
        if cells.first?.isEmpty == true {
            cells.removeFirst()
        }
        if cells.last?.isEmpty == true {
            cells.removeLast()
        }
        return cells
    }

    private static func sides(_ row: [String]) -> [Side]? {
        let dashes = row.allSatisfy { cell in cell.contains("-") && cell.allSatisfy { "-: ".contains($0) } }
        guard dashes, !row.isEmpty else { return nil }
        return row.map { cell in
            switch (cell.hasPrefix(":"), cell.hasSuffix(":")) {
            case (true, true): .center
            case (false, true): .trailing
            default: .leading
            }
        }
    }
}

private struct ProseTableView: View {
    private static let widestCell: CGFloat = 280

    @Environment(\.theme) private var theme
    let table: ProseTable

    var body: some View {
        ScrollView(.horizontal) {
            Grid(alignment: .topLeading, horizontalSpacing: 18, verticalSpacing: 7) {
                if !table.header.isEmpty {
                    GridRow {
                        ForEach(0 ..< table.columns, id: \.self) { column in
                            cell(table.header, column).fontWeight(.semibold)
                        }
                    }
                    Rectangle().fill(theme.colors.hairline).frame(height: 0.5).gridCellUnsizedAxes(.horizontal)
                }
                ForEach(table.rows.indices, id: \.self) { row in
                    GridRow {
                        ForEach(0 ..< table.columns, id: \.self) { column in
                            cell(table.rows[row], column)
                        }
                    }
                }
            }
            .padding(.vertical, 2)
        }
        .scrollIndicators(.never)
    }

    private func cell(_ row: [String], _ column: Int) -> some View {
        let alignment: Alignment = switch table.side(of: column) {
        case .leading: .leading
        case .center: .center
        case .trailing: .trailing
        }
        return Text(ProjectMessageText.formatted(column < row.count ? row[column] : ""))
            .appTextStyle(.body).foregroundStyle(theme.colors.ink).messageTextSelection()
            .tint(theme.colors.accentStrong)
            .multilineTextAlignment(alignment == .trailing ? .trailing : alignment == .center ? .center : .leading)
            .frame(maxWidth: Self.widestCell, alignment: alignment)
            .fixedSize(horizontal: false, vertical: true)
            .gridColumnAlignment(alignment.horizontal)
    }
}

struct FollowsEnd: ViewModifier {
    @Binding var following: Bool
    @Binding var position: ScrollPosition
    @Binding var viewport: CGFloat
    @State private var phase = ScrollPhase.idle
    @State private var aim: CGFloat?

    func body(content: Content) -> some View {
        content
            .scrollPosition($position)
            .defaultScrollAnchor(.bottom, for: .initialOffset)
            .onScrollGeometryChange(for: ScrollGeometry.self) { $0 } action: { old, new in
                viewport = new.containerSize.height
                let end = max(0, new.contentSize.height - new.containerSize.height)
                let offset = new.contentOffset.y + new.contentInsets.top
                if new.contentSize != old.contentSize || new.containerSize != old.containerSize || new.contentInsets != old.contentInsets {
                    if following, abs(offset - end) > 0.5 {
                        scroll(to: end)
                    }
                } else if following, let aim, abs(offset - aim) < 0.5, abs(offset - end) > 0.5 {
                    // Re-scroll if our previous scroll target landed after content grew further.
                    scroll(to: end)
                } else if offset >= end - 40 {
                    following = true
                } else if old.contentOffset.y - new.contentOffset.y > Self.leaves(phase) {
                    following = false
                }
            }
            .onScrollPhaseChange { _, new in phase = new }
    }

    private func scroll(to end: CGFloat) {
        aim = end
        withTransaction(\.disablesAnimations, true) { position.scrollTo(y: end) }
    }

    /// Minimum upward delta to disengage auto-follow. Touch scrolling always enters an active
    /// phase, whereas macOS wheel scrolling fires during `.idle`. Ignoring `.idle` on iOS keeps
    /// keyboard animations and programmatic scrolls from breaking follow mode.
    private static func leaves(_ phase: ScrollPhase) -> CGFloat {
        switch phase {
        case .tracking, .interacting, .decelerating:
            0.5
        case .idle:
            #if os(iOS)
                .infinity
            #else
                8
            #endif
        default:
            .infinity
        }
    }
}

extension View {
    func followsEnd(_ following: Binding<Bool>, position: Binding<ScrollPosition>, viewport: Binding<CGFloat>) -> some View {
        modifier(FollowsEnd(following: following, position: position, viewport: viewport))
    }
}

struct LatestKey: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) { Label { Text("Latest", bundle: .module) } icon: { Image(systemName: "arrow.down") } }
            .buttonStyle(.kodosi(.secondary, size: .small)).padding(8)
            .transition(.move(edge: .bottom).combined(with: .opacity))
    }
}
