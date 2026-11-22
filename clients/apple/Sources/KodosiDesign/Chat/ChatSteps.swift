import KodosiBridge
import SwiftUI

/// The work of a turn as one line: what the steps did, and the steps that did not go as the agent
/// wanted. The line ends with its status sign, and it is there from the first step, so a person can
/// open a step while it runs.
struct ChatWork: View {
    @Environment(\.theme) private var theme
    let items: [ChatItem]
    let hold: () -> Void
    @State private var open = false

    var body: some View {
        let tools = items.tools
        let ended = tools.filter { !$0.state.isOpen }
        let done = ended.filter { $0.state == .done }
        VStack(alignment: .leading, spacing: 0) {
            Button {
                hold()
                open.toggle()
            } label: {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    StepSymbol(kind: Self.main(tools))
                    Group {
                        if !done.isEmpty {
                            Text(ChatWords.summary(done))
                        } else if ended.isEmpty {
                            Text("\(tools.count) steps", bundle: .module)
                        }
                    }
                    .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).lineLimit(2)
                    .multilineTextAlignment(.leading)
                    ForEach(ChatWords.setbacks(ended).filter { $0.state != .failed }, id: \.state) { setback in
                        Text(verbatim: setback.words).appTextStyle(.footnote).foregroundStyle(theme.colors.inkFaint).lineLimit(1).fixedSize()
                    }
                    ZStack {
                        if ended.contains(where: { $0.state == .failed }) {
                            StatusSign(form: .failed, size: 11).transition(AnyTransition.pop)
                        } else if !tools.isEmpty, done.count == tools.count {
                            StatusSign(form: .done, size: 11).transition(AnyTransition.pop)
                        }
                    }
                    Image(systemName: "chevron.right").font(mac: 8, ios: .caption2, weight: .bold)
                        .rotationEffect(.degrees(open ? 90 : 0)).foregroundStyle(theme.colors.inkFaint)
                        .accessibilityHidden(true)
                }
                .controlHeight(26).touchTarget(Rectangle())
            }
            .buttonStyle(.plain)
            .help(open ? Text("Hide the steps", bundle: .module) : Text("Show the steps", bundle: .module))
            .accessibilityHint(open ? Text("Hide the steps", bundle: .module) : Text("Show the steps", bundle: .module))
            if open {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(items) { item in
                        ChatStepLine(item: item, hold: hold).transition(AnyTransition.unfold)
                    }
                }
                .padding(.bottom, 4)
                .transition(.opacity)
            }
        }
        .animation(theme.motion.snappy, value: open)
        .animation(theme.motion.spring, value: items.map(\.id))
        .animation(theme.motion.snappy, value: ended.count)
    }

    /// The kind that most steps of the turn have.
    private static func main(_ tools: [ChatTool]) -> ChatToolKind {
        var counts: [ChatToolKind: Int] = [:]
        for tool in tools {
            counts[tool.kind, default: 0] += 1
        }
        return tools.map(\.kind).max { counts[$0, default: 0] < counts[$1, default: 0] } ?? .other
    }
}

/// The plain symbol of a kind of step: no tile, no outline.
struct StepSymbol: View {
    @Environment(\.theme) private var theme
    let kind: ChatToolKind

    var body: some View {
        Image(systemName: ChatWords.symbol(kind)).font(mac: 10, ios: .caption, weight: .semibold)
            .foregroundStyle(theme.colors.inkFaint)
            .frame(width: 15)
            .accessibilityHidden(true)
    }
}

struct ChatStepLine: View {
    @Environment(\.theme) private var theme
    let item: ChatItem
    let hold: () -> Void
    @State private var open = false

    var body: some View {
        switch item.body {
        case let .tool(tool):
            VStack(alignment: .leading, spacing: 4) {
                if ChatStepDetail.shows(tool) {
                    Button {
                        hold()
                        open.toggle()
                    } label: {
                        line(tool, opens: true).touchTarget(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .help(tool.target)
                } else {
                    line(tool, opens: false).help(tool.target)
                }
                if open {
                    ChatStepDetail(tool: tool).padding(.leading, 21).padding(.bottom, 6).transition(AnyTransition.unfold)
                }
            }
            .animation(theme.motion.snappy, value: open)
        case let .text(text, _):
            Text(ProjectMessageText.formatted(text)).appTextStyle(.footnote).lineSpacing(2).foregroundStyle(theme.colors.inkMuted)
                .textSelection(.enabled).padding(.vertical, 4).padding(.leading, 21)
        default:
            EmptyView()
        }
    }

    private func line(_ tool: ChatTool, opens: Bool) -> some View {
        HStack(spacing: 6) {
            StepSymbol(kind: tool.kind)
            Text(verbatim: ChatWords.step(tool))
                .font(mac: 11.5, ios: .footnote, design: Self.code(tool) ? .monospaced : .default)
                .foregroundStyle(tool.state.isOpen ? theme.colors.ink : theme.colors.inkMuted)
                .lineLimit(1).truncationMode(.middle)
            ChatStepState(state: tool.state)
            if opens {
                Image(systemName: "chevron.right").font(mac: 8, ios: .caption2, weight: .bold)
                    .rotationEffect(.degrees(open ? 90 : 0)).foregroundStyle(theme.colors.inkFaint)
                    .accessibilityHidden(true)
            }
            Spacer(minLength: 0)
        }
        #if os(iOS)
        .frame(minHeight: 30)
        #else
        .frame(minHeight: 24)
        #endif
        .contentShape(Rectangle())
    }

    /// A command, a file and a search say themselves in the letters of code.
    private static func code(_ tool: ChatTool) -> Bool {
        [.command, .read, .edit, .write, .search].contains(tool.kind)
    }
}

struct ChatStepState: View {
    @Environment(\.theme) private var theme
    let state: ChatToolState

    var body: some View {
        Group {
            switch state {
            case .done:
                StatusSign(form: .done, size: 10)
            case .failed:
                StatusSign(form: .failed, size: 10)
            case .denied:
                word(Text("denied", bundle: .module))
            case .stopped:
                word(Text("stopped", bundle: .module))
            case .preparing, .asking, .running:
                EmptyView()
            }
        }
        .transition(AnyTransition.pop)
    }

    private func word(_ text: Text) -> some View {
        text.appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
    }
}

struct ChatStepDetail: View {
    @Environment(\.theme) private var theme
    let tool: ChatTool

    static func shows(_ tool: ChatTool) -> Bool {
        switch tool.detail {
        case let .command(command, output, background): !output.isEmpty || background || command != tool.target
        case .change, .plan, .agent: true
        case let .output(text): !text.isEmpty
        case let .question(questions, _): !questions.isEmpty
        case nil: false
        }
    }

    var body: some View {
        switch tool.detail {
        case let .command(command, output, background):
            VStack(alignment: .leading, spacing: 6) {
                if command != tool.target || !output.isEmpty {
                    CodeTile(text: command, output: output)
                }
                if background {
                    Label { Text("in the background", bundle: .module) } icon: { Image(systemName: "arrow.triangle.2.circlepath") }
                        .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
                }
            }
        case let .change(path, added, removed, lines):
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    Text(verbatim: HostPath.folderName(path)).appTextStyle(.monoCaption).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
                    Text(verbatim: "+\(added)").appTextStyle(.monoCaption).foregroundStyle(theme.colors.ready)
                    Text(verbatim: "−\(removed)").appTextStyle(.monoCaption).foregroundStyle(theme.colors.danger)
                }
                .help(path)
                ChangeLines(lines: lines, most: 40)
            }
        case let .output(text):
            CodeTile(text: "", output: text)
        case let .question(questions, answers):
            VStack(alignment: .leading, spacing: 6) {
                ForEach(questions.indices, id: \.self) { index in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(questions[index].question).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        if index < answers.count {
                            Text(answers[index]).appTextStyle(.footnote).fontWeight(.semibold).foregroundStyle(theme.colors.ink)
                        }
                    }
                }
            }
            .textSelection(.enabled)
        case let .plan(text):
            AgentProse(text: text, live: false).padding(12).frame(maxWidth: .infinity, alignment: .leading)
                .raised(Radius.lg, fill: theme.colors.raised)
        case let .agent(name, steps, doing, said):
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    if !name.isEmpty {
                        Text(verbatim: name).appTextStyle(.footnote).fontWeight(.semibold).foregroundStyle(theme.colors.ink)
                    }
                    Text("\(steps) steps", bundle: .module).appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
                }
                if let doing {
                    HStack(spacing: 6) {
                        StepSymbol(kind: doing.kind)
                        Text(verbatim: doing.target).appTextStyle(.monoCaption).foregroundStyle(theme.colors.inkMuted)
                            .lineLimit(1).truncationMode(.middle)
                    }
                }
                if !said.isEmpty {
                    Text(ProjectMessageText.formatted(said)).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        .lineLimit(6).textSelection(.enabled)
                }
            }
        case nil:
            EmptyView()
        }
    }
}

/// Code that a step ran and what it printed, sunk into the chat that shows it. What a command
/// printed starts at its end, where a result or a fault is.
struct CodeTile: View {
    @Environment(\.theme) private var theme
    let text: String
    var output = ""
    var most: CGFloat = 180

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 6) {
                if !text.isEmpty {
                    Text(verbatim: text).foregroundStyle(theme.colors.ink)
                }
                if !output.isEmpty {
                    Text(verbatim: output).foregroundStyle(theme.colors.inkMuted)
                }
            }
            .appTextStyle(.monoCaption).textSelection(.enabled)
            .frame(maxWidth: .infinity, alignment: .leading).padding(10)
        }
        .defaultScrollAnchor(output.isEmpty ? .top : .bottom)
        .frame(maxHeight: most).fixedSize(horizontal: false, vertical: true)
        .well(Radius.md)
    }
}

/// The lines of a change to a file, as the agent sends them, sunk into the chat that shows them.
struct ChangeLines: View {
    @Environment(\.theme) private var theme
    let lines: [ChangeLine]
    let most: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(lines.prefix(most).enumerated()), id: \.offset) { _, line in
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(verbatim: line.sign == .added ? "+" : line.sign == .removed ? "−" : " ")
                    Text(verbatim: line.text).lineLimit(4).truncationMode(.tail)
                }
                .foregroundStyle(line.sign == .added ? theme.colors.ready : line.sign == .removed ? theme.colors.danger : theme.colors.inkFaint)
            }
            if lines.count > most {
                Text(verbatim: "…").foregroundStyle(theme.colors.inkFaint)
            }
        }
        .appTextStyle(.monoCaption)
        .textSelection(.enabled)
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(10)
        .well(Radius.md)
    }
}
