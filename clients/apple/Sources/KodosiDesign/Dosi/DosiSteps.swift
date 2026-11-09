import KodosiBridge
import SwiftUI

struct DosiThought: View {
    @Environment(\.theme) private var theme
    let text: String
    let live: Bool
    let seconds: Int?
    @State private var open = false

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Button { open.toggle() } label: {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.right").font(mac: 8, ios: .caption2, weight: .bold)
                        .rotationEffect(.degrees(open ? 90 : 0))
                    Text(live ? String(localized: "thinking", bundle: .module)
                        : seconds.map { String(localized: "thought for \($0)s", bundle: .module) } ?? String(localized: "thought", bundle: .module))
                        .font(mac: 11, ios: .caption, design: .monospaced)
                }
                .foregroundStyle(theme.colors.inkFaint)
                .controlHeight(18).touchTarget(Rectangle())
            }
            .buttonStyle(.plain).disabled(text.isEmpty)
            if open, !text.isEmpty {
                Text(text).font(mac: 11.5, ios: .footnote).lineSpacing(2).foregroundStyle(theme.colors.inkFaint).textSelection(.enabled)
                    .padding(.leading, 12)
                    .overlay(alignment: .leading) { Rectangle().fill(theme.colors.lifted).frame(width: 1) }
                    .padding(.leading, 4)
                    .transition(AnyTransition.unfold)
            }
        }
        .animation(theme.motion.snappy, value: open)
    }
}

struct DosiThinking: View {
    @Environment(\.theme) private var theme

    var body: some View {
        Text("thinking", bundle: .module).font(mac: 11, ios: .caption, design: .monospaced).foregroundStyle(theme.colors.inkFaint)
            .padding(.leading, 14).controlHeight(18)
            .accessibilityIdentifier("dosi.thinking")
    }
}

struct DosiToolStatus: View {
    @Environment(\.theme) private var theme
    let status: DosiTool.Status

    var body: some View {
        Group {
            switch status {
            case .running:
                Spinner(size: 10)
            case .done:
                Image(systemName: "checkmark").font(mac: 9, ios: .caption2, weight: .bold).foregroundStyle(theme.colors.ready)
            case .failed:
                Image(systemName: "exclamationmark.triangle.fill").font(mac: 9, ios: .caption2, weight: .semibold)
                    .foregroundStyle(theme.colors.caution)
            }
        }
        .transition(AnyTransition.pop)
    }
}

struct DosiMini: View {
    @Environment(DosiPlace.self) private var place
    @Environment(\.theme) private var theme
    let target: DosiTarget?
    let action: DosiTool.Action

    var body: some View {
        switch target?.kind {
        case .terminal:
            if let target, let session = place.terminal(target.value) {
                AgentMark(session: session, size: 14)
            } else {
                glyph("apple.terminal")
            }
        case .room:
            RoomSigil(key: target.map { place.room($0.value)?.id ?? $0.value } ?? "", size: 14)
        case .terminals where !place.sessions().isEmpty:
            HStack(spacing: -5) {
                ForEach(place.sessions().prefix(3)) { session in
                    AgentMark(session: session, size: 14)
                        .overlay { RoundedRectangle(cornerRadius: 4.2, style: .continuous).stroke(theme.colors.ground, lineWidth: 1.5) }
                }
            }
        case .rooms where !place.rooms().isEmpty:
            HStack(spacing: -5) {
                ForEach(place.rooms().prefix(3)) { mission in
                    RoomSigil(key: mission.id, size: 14)
                        .overlay { RoundedRectangle(cornerRadius: 4.2, style: .continuous).stroke(theme.colors.ground, lineWidth: 1.5) }
                }
            }
        case .agent:
            AgentMark(kind: AgentKind(program: target?.value ?? ""), size: 14)
        case .url:
            glyph("globe")
        case .file:
            glyph("doc.text")
        case .query:
            glyph("magnifyingglass")
        case .command:
            glyph("chevron.right")
        default:
            glyph(DosiWords.deed(action).symbol)
        }
    }

    private func glyph(_ symbol: String) -> some View {
        Image(systemName: symbol).font(.system(size: 7.5, weight: .bold)).foregroundStyle(theme.colors.inkMuted)
            .frame(width: 14, height: 14)
            .background(theme.colors.lifted, in: RoundedRectangle(cornerRadius: 4, style: .continuous))
    }
}

struct DosiReceipt: View {
    @Environment(DosiPlace.self) private var place
    @Environment(\.theme) private var theme
    let tool: DosiTool
    @State private var open = false

    private var openable: Bool {
        tool.target.map { [.terminal, .room, .rooms, .terminals, .url].contains($0.kind) } ?? false
    }

    var body: some View {
        let running = tool.status == .running
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 2) {
                Button {
                    if let target = tool.target, openable {
                        place.open(target)
                    } else if tool.output != nil {
                        open.toggle()
                    }
                } label: {
                    HStack(spacing: 7) {
                        DosiMini(target: tool.target, action: tool.action)
                        Text(DosiWords.receipt(tool)).font(mac: 11, ios: .caption, design: .monospaced)
                            .foregroundStyle(running ? theme.colors.ink : theme.colors.inkMuted)
                            .lineLimit(1).truncationMode(.middle)
                        DosiToolStatus(status: tool.status)
                    }
                    .padding(.leading, 5).padding(.trailing, 9).controlHeight(22)
                    .background(theme.colors.surface, in: Capsule())
                    .touchTarget(Capsule())
                }
                .buttonStyle(PressScaleStyle(scale: 0.97))
                .help(tool.target?.value ?? "")
                if tool.output != nil {
                    Button { open.toggle() } label: {
                        Image(systemName: "chevron.right").font(mac: 8, ios: .caption2, weight: .bold)
                            .rotationEffect(.degrees(open ? 90 : 0)).foregroundStyle(theme.colors.inkFaint)
                            .frame(width: 18).controlHeight(22).touchTarget(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .help(open ? Text("Hide the output", bundle: .module) : Text("Show the output", bundle: .module))
                    .accessibilityLabel(open ? Text("Hide the output", bundle: .module) : Text("Show the output", bundle: .module))
                }
            }
            if open, let output = tool.output {
                Text(output).appTextStyle(.monoCaption).foregroundStyle(theme.colors.inkMuted).textSelection(.enabled)
                    .lineLimit(12).frame(maxWidth: .infinity, alignment: .leading)
                    .padding(8).well(Radius.md).transition(AnyTransition.unfold)
            }
        }
        .animation(theme.motion.snappy, value: open)
        .animation(theme.motion.snappy, value: tool.status)
    }
}

struct DosiToolGroup: View {
    @Environment(\.theme) private var theme
    let items: [DosiItem]
    @State private var open = false

    private var tools: [DosiTool] {
        items.compactMap { item in
            if case let .tool(tool) = item.body {
                return tool
            }
            return nil
        }
    }

    var body: some View {
        let tools = tools
        let running = tools.contains { $0.status == .running }
        let failed = tools.contains { $0.status == .failed }
        let shown = open || running
        VStack(alignment: .leading, spacing: 2) {
            Button { open.toggle() } label: {
                HStack(spacing: 7) {
                    HStack(spacing: -4) {
                        ForEach(Array(marks(tools).enumerated()), id: \.offset) { _, tool in
                            DosiMini(target: tool.target, action: tool.action)
                                .overlay { RoundedRectangle(cornerRadius: 4.2, style: .continuous).stroke(theme.colors.ground, lineWidth: 1.5) }
                        }
                    }
                    Text(DosiWords.summary(tools)).font(mac: 11, ios: .caption, design: .monospaced)
                        .foregroundStyle(running ? theme.colors.ink : theme.colors.inkMuted).lineLimit(1)
                    DosiToolStatus(status: running ? .running : failed ? .failed : .done)
                    Image(systemName: "chevron.right").font(mac: 8, ios: .caption2, weight: .bold)
                        .rotationEffect(.degrees(shown ? 90 : 0)).foregroundStyle(theme.colors.inkFaint)
                }
                .controlHeight(22).touchTarget(Rectangle())
            }
            .buttonStyle(.plain)
            .help(shown ? Text("Hide the steps", bundle: .module) : Text("Show the steps", bundle: .module))
            .accessibilityHint(shown ? Text("Hide the steps", bundle: .module) : Text("Show the steps", bundle: .module))
            if shown {
                VStack(alignment: .leading, spacing: 0) {
                    ForEach(items) { item in
                        if case let .tool(tool) = item.body {
                            DosiToolLine(tool: tool).transition(AnyTransition.unfold)
                        }
                    }
                }
                .padding(.leading, 12)
                .overlay(alignment: .leading) { Rectangle().fill(theme.colors.lifted).frame(width: 1) }
                .padding(.leading, 6)
                .transition(.opacity)
            }
        }
        .animation(theme.motion.snappy, value: shown)
        .animation(theme.motion.spring, value: items.map(\.id))
    }

    private func marks(_ tools: [DosiTool]) -> [DosiTool] {
        var seen: [DosiTarget?] = []
        var marks: [DosiTool] = []
        for tool in tools where !seen.contains(tool.target) && marks.count < 5 {
            seen.append(tool.target)
            marks.append(tool)
        }
        return marks
    }
}

struct DosiToolLine: View {
    @Environment(DosiPlace.self) private var place
    @Environment(\.theme) private var theme
    let tool: DosiTool

    var body: some View {
        HStack(spacing: 7) {
            DosiMini(target: tool.target, action: tool.action)
            Text(DosiWords.receipt(tool)).font(mac: 11, ios: .caption, design: .monospaced)
                .foregroundStyle(tool.status == .running ? theme.colors.ink : theme.colors.inkMuted)
                .lineLimit(1).truncationMode(.middle)
            Spacer(minLength: 4)
            DosiToolStatus(status: tool.status)
        }
        #if os(iOS)
        .frame(minHeight: 44)
        #else
        .frame(height: 22)
        #endif
        .contentShape(Rectangle())
        .onTapGesture {
            if let target = tool.target {
                place.open(target)
            }
        }
        .help(tool.target?.value ?? "")
        .animation(theme.motion.snappy, value: tool.status)
    }
}
