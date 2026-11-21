import KodosiBridge
import SwiftUI

/// The work of a turn as one line. While the turn runs, it counts the steps that ended, and the
/// live line under the conversation says what runs.
struct ChatWork: View {
    @Environment(\.theme) private var theme
    let items: [ChatItem]
    let live: Bool
    let hold: () -> Void
    @State private var open = false

    var body: some View {
        let tools = items.tools
        let ended = tools.filter { !$0.state.isOpen }
        if !ended.isEmpty || open {
            VStack(alignment: .leading, spacing: 4) {
                Button {
                    hold()
                    open.toggle()
                } label: {
                    HStack(spacing: 7) {
                        HStack(spacing: -4) {
                            ForEach(kinds(ended), id: \.self) { kind in
                                StepGlyph(symbol: ChatWords.symbol(kind))
                                    .overlay { RoundedRectangle(cornerRadius: 4.2, style: .continuous).stroke(theme.colors.terminal, lineWidth: 1.5) }
                            }
                        }
                        Text(ChatWords.summary(ended)).font(mac: 11, ios: .caption, design: .monospaced)
                            .foregroundStyle(theme.colors.inkMuted).lineLimit(1)
                            .contentTransition(.interpolate)
                        ForEach(ChatWords.setbacks(ended), id: \.state) { setback in
                            HStack(spacing: 3) {
                                if setback.state == .failed {
                                    Image(systemName: "exclamationmark.triangle.fill").font(mac: 9, ios: .caption2, weight: .semibold)
                                        .accessibilityHidden(true)
                                }
                                Text(verbatim: setback.words).font(mac: 11, ios: .caption, design: .monospaced).lineLimit(1)
                            }
                            .foregroundStyle(setback.state == .failed ? theme.colors.caution : theme.colors.inkFaint)
                            .fixedSize()
                        }
                        Image(systemName: "chevron.right").font(mac: 8, ios: .caption2, weight: .bold)
                            .rotationEffect(.degrees(open ? 90 : 0)).foregroundStyle(theme.colors.inkFaint)
                            .accessibilityHidden(true)
                    }
                    .controlHeight(22).touchTarget(Rectangle())
                }
                .buttonStyle(.plain)
                .help(open ? Text("Hide the steps", bundle: .module) : Text("Show the steps", bundle: .module))
                .accessibilityHint(open ? Text("Hide the steps", bundle: .module) : Text("Show the steps", bundle: .module))
                if open {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(items) { item in
                            ChatStepLine(item: item, hold: hold).transition(AnyTransition.unfold)
                        }
                    }
                    .padding(.leading, 12)
                    .overlay(alignment: .leading) { Rectangle().fill(theme.colors.lifted).frame(width: 1) }
                    .padding(.leading, 6)
                    .transition(.opacity)
                }
            }
            .animation(theme.motion.snappy, value: open)
            .animation(theme.motion.spring, value: items.map(\.id))
        }
    }

    private func kinds(_ tools: [ChatTool]) -> [ChatToolKind] {
        var kinds: [ChatToolKind] = []
        for tool in tools where !kinds.contains(tool.kind) && kinds.count < 4 {
            kinds.append(tool.kind)
        }
        return kinds
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
                Button {
                    hold()
                    open.toggle()
                } label: {
                    HStack(spacing: 7) {
                        StepGlyph(symbol: ChatWords.symbol(tool.kind))
                        Text(verbatim: ChatWords.step(tool)).font(mac: 11, ios: .caption, design: .monospaced)
                            .foregroundStyle(tool.state.isOpen ? theme.colors.ink : theme.colors.inkMuted)
                            .lineLimit(1).truncationMode(.middle)
                        ChatStepState(state: tool.state)
                        if ChatStepDetail.shows(tool) {
                            Image(systemName: "chevron.right").font(mac: 8, ios: .caption2, weight: .bold)
                                .rotationEffect(.degrees(open ? 90 : 0)).foregroundStyle(theme.colors.inkFaint)
                                .accessibilityHidden(true)
                        }
                        Spacer(minLength: 0)
                    }
                    #if os(iOS)
                    .frame(minHeight: 44)
                    #else
                    .frame(minHeight: 22)
                    #endif
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(!ChatStepDetail.shows(tool))
                .help(tool.target)
                if open {
                    ChatStepDetail(tool: tool).transition(AnyTransition.unfold)
                }
            }
            .animation(theme.motion.snappy, value: open)
        case let .text(text, _):
            Text(ProjectMessageText.formatted(text)).appTextStyle(.footnote).lineSpacing(2).foregroundStyle(theme.colors.inkMuted)
                .textSelection(.enabled).padding(.vertical, 4)
        default:
            EmptyView()
        }
    }
}

struct ChatStepState: View {
    @Environment(\.theme) private var theme
    let state: ChatToolState

    var body: some View {
        Group {
            switch state {
            case .done:
                Image(systemName: "checkmark").font(mac: 9, ios: .caption2, weight: .bold).foregroundStyle(theme.colors.ready)
                    .accessibilityLabel(Text("Done", bundle: .module))
            case .failed:
                Image(systemName: "exclamationmark.triangle.fill").font(mac: 9, ios: .caption2, weight: .semibold)
                    .foregroundStyle(theme.colors.caution)
                    .accessibilityLabel(Text("Failed", bundle: .module))
            case .denied:
                word(Text("denied", bundle: .module))
            case .stopped:
                word(Text("stopped", bundle: .module))
            case .preparing, .asking, .running, .other:
                EmptyView()
            }
        }
        .transition(AnyTransition.pop)
    }

    private func word(_ text: Text) -> some View {
        text.font(mac: 10.5, ios: .caption2, design: .monospaced).foregroundStyle(theme.colors.inkFaint)
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
        case .other, nil: false
        }
    }

    var body: some View {
        switch tool.detail {
        case let .command(command, output, background):
            VStack(alignment: .leading, spacing: 6) {
                if command != tool.target {
                    well(command, color: theme.colors.ink)
                }
                if !output.isEmpty {
                    well(output, color: theme.colors.inkMuted)
                }
                if background {
                    Label { Text("in the background", bundle: .module) } icon: { Image(systemName: "arrow.triangle.2.circlepath") }
                        .font(mac: 10.5, ios: .caption2, design: .monospaced).foregroundStyle(theme.colors.inkFaint)
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
            well(text, color: theme.colors.inkMuted)
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
            AgentProse(text: text, live: false).padding(10).well(Radius.md)
        case let .agent(name, steps, doing, said):
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    if !name.isEmpty {
                        Text(verbatim: name).appTextStyle(.monoCaption).foregroundStyle(theme.colors.ink)
                    }
                    Text("\(steps) steps", bundle: .module).appTextStyle(.monoCaption).foregroundStyle(theme.colors.inkFaint)
                }
                if let doing {
                    HStack(spacing: 6) {
                        StepGlyph(symbol: ChatWords.symbol(doing.kind))
                        Text(verbatim: doing.target).appTextStyle(.monoCaption).foregroundStyle(theme.colors.inkMuted)
                            .lineLimit(1).truncationMode(.middle)
                    }
                }
                if !said.isEmpty {
                    Text(ProjectMessageText.formatted(said)).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        .lineLimit(6).textSelection(.enabled)
                }
            }
        case .other, nil:
            EmptyView()
        }
    }

    private func well(_ text: String, color: Color) -> some View {
        Text(verbatim: text).appTextStyle(.monoCaption).foregroundStyle(color).textSelection(.enabled)
            .lineLimit(12).frame(maxWidth: .infinity, alignment: .leading)
            .padding(8).well(Radius.md)
    }
}

/// The lines of a change to a file, as the agent sends them.
struct ChangeLines: View {
    @Environment(\.theme) private var theme
    let lines: [ChangeLine]
    let most: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(lines.prefix(most).enumerated()), id: \.offset) { _, line in
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text(verbatim: line.sign == .added ? "+" : line.sign == .removed ? "−" : " ")
                    Text(verbatim: line.text).lineLimit(1).truncationMode(.tail)
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
        .padding(8).well(Radius.md)
    }
}
