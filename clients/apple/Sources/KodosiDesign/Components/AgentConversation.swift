import SwiftUI

struct PersonBubble: View {
    @Environment(\.theme) private var theme
    let text: String

    var body: some View {
        HStack(spacing: 0) {
            Spacer(minLength: 56)
            Text(ProjectMessageText.formatted(text))
                .appTextStyle(.body).lineSpacing(2).foregroundStyle(theme.colors.ink).textSelection(.enabled)
                .projectBubble()
        }
    }
}

/// An agent's answer. Agents write Markdown: a block of code sits in a well, a heading stands out,
/// and the other lines keep their breaks with their inline marks.
struct AgentProse: View {
    @Environment(\.theme) private var theme
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
                        Text(ProjectMessageText.formatted(lines))
                            .appTextStyle(.body).lineSpacing(3).foregroundStyle(theme.colors.ink).textSelection(.enabled)
                            .tint(theme.colors.accentStrong)
                        if last, live {
                            CursorBlock(width: 6, height: 14, blink: .typing).transition(.opacity)
                        }
                    }
                case let .heading(heading):
                    Text(ProjectMessageText.formatted(heading))
                        .appTextStyle(.subhead).foregroundStyle(theme.colors.ink).textSelection(.enabled)
                        .padding(.top, index == 0 ? 0 : 3)
                case let .code(code):
                    ScrollView(.horizontal) {
                        Text(verbatim: code).appTextStyle(.monoCaption).foregroundStyle(theme.colors.ink).textSelection(.enabled)
                            .fixedSize().padding(.horizontal, 10).padding(.vertical, 8)
                    }
                    .scrollIndicators(.never)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .well(Radius.md)
                case let .table(table):
                    ScrollView(.horizontal) {
                        Text(verbatim: table).appTextStyle(.monoCaption).foregroundStyle(theme.colors.inkMuted).textSelection(.enabled)
                            .fixedSize()
                    }
                    .scrollIndicators(.never)
                }
            }
        }
        .animation(theme.motion.fade, value: live)
    }
}

enum ProseBlock: Equatable {
    case lines(String)
    case heading(String)
    case code(String)
    case table(String)

    /// A fence that is still open while the agent writes holds the rest.
    static func blocks(of text: String) -> [Self] {
        var blocks: [Self] = []
        var lines: [Substring] = []
        var code: [Substring]?
        var table: [Substring] = []
        func flush() {
            if !table.isEmpty {
                blocks.append(.table(table.joined(separator: "\n")))
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

/// A conversation follows its newest line while the person is at the end, and stays where the
/// person scrolled to.
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
                    // A scroll of the view lands after the content changed again: its end then is
                    // not the end now.
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

    /// A person's scroll on a touch screen always has a phase, while a mouse wheel moves the
    /// Mac's view in the idle phase. The moves of the keyboard and of the app's own scrolls
    /// must not stop the following.
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
    @Environment(\.theme) private var theme
    let action: () -> Void

    var body: some View {
        Button(action: action) { Label { Text("Latest", bundle: .module) } icon: { Image(systemName: "arrow.down") } }
            .buttonStyle(.kodosi(.secondary, size: .small)).padding(8)
            .transition(.move(edge: .bottom).combined(with: .opacity))
    }
}
