import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

enum DosiShelfFit {
    static let spacing: CGFloat = 4
    static let nameWidth: CGFloat = 112

    static func chip(_ name: String, sign: CGFloat?) -> CGFloat {
        let font = NSFont.systemFont(ofSize: 11.5, weight: .semibold)
        let text = min(ceil((name as NSString).size(withAttributes: [.font: font]).width), nameWidth)
        return 37 + text + (sign.map { 6 + $0 } ?? 0)
    }

    static func more(_ hidden: Int) -> CGFloat {
        let font = NSFont.monospacedDigitSystemFont(ofSize: 11.5, weight: .semibold)
        return 28 + ceil(("\(hidden)" as NSString).size(withAttributes: [.font: font]).width)
    }

    static func shown(_ widths: [CGFloat], in room: CGFloat, more: (Int) -> CGFloat = DosiShelfFit.more) -> Int {
        guard widths.reduce(0, +) + spacing * CGFloat(max(widths.count - 1, 0)) > room else { return widths.count }
        var used: CGFloat = 0
        var count = 0
        while count < widths.count, used + widths[count] + spacing + more(widths.count - count - 1) <= room {
            used += widths[count] + spacing
            count += 1
        }
        return count
    }
}

struct DosiShelf: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    private var terminals: [RuntimeSession] {
        deps.sessions.enumerated().sorted { left, right in
            let ranks = (need(left.element), need(right.element))
            return ranks.0 == ranks.1 ? left.offset < right.offset : ranks.0 < ranks.1
        }
        .map(\.element)
    }

    private var rooms: [MissionEntry] {
        deps.missions.enumerated().sorted { left, right in
            left.element.unread == right.element.unread ? left.offset < right.offset : left.element.unread
        }
        .map(\.element)
    }

    var body: some View {
        let terminals = terminals
        let rooms = rooms
        VStack(spacing: 4) {
            DosiShelfRow(items: terminals, widths: terminals.map { chipWidth(of: $0) }, estimate: estimate) { session, order, width in
                DosiTerminalChip(session: session, order: order, width: width).transition(Self.chip)
            } more: { hidden, order in
                DosiMoreTerminals(sessions: hidden, order: order, needs: hidden.contains { need($0) <= 1 }).transition(Self.chip)
            } trailing: {
                DosiAddMark(title: "New terminal", identifier: "dosi.marks.new") { deps.openLauncher() }
                    .fold(FoldPart.newTerminal)
            }
            if !rooms.isEmpty {
                DosiShelfRow(items: rooms, widths: rooms.map { chipWidth(of: $0) }, estimate: estimate) { mission, order, width in
                    DosiRoomChip(mission: mission, order: order, width: width).transition(Self.chip)
                } more: { hidden, order in
                    DosiMoreRooms(missions: hidden, order: order).transition(Self.chip)
                } trailing: {
                    DosiAddMark(title: "New room", identifier: "dosi.marks.room.new") { deps.workbench.showsNewRoom = true }
                }
                .transition(AnyTransition.unfold)
            }
        }
        .padding(5)
        .background {
            RaisedBackground(shape: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous), fill: theme.colors.surface, elevation: .resting)
                .arrive(delay: 0.08, rise: 0)
        }
        .animation(theme.motion.spring, value: terminals.map(\.id))
        .animation(theme.motion.spring, value: rooms.map(\.id))
    }

    private var estimate: CGFloat {
        deps.workbench.dosiWidth - 68
    }

    private static var chip: AnyTransition {
        AnyTransition.pop
    }

    private func chipWidth(of session: RuntimeSession) -> CGFloat {
        DosiShelfFit.chip(session.name, sign: deps.sign(of: session) == nil ? nil : 14)
    }

    private func chipWidth(of mission: MissionEntry) -> CGFloat {
        DosiShelfFit.chip(mission.name, sign: mission.unread ? 8 : nil)
    }

    private func need(_ session: RuntimeSession) -> Int {
        switch deps.sign(of: session) {
        case .hand?, .question?, .key?: 0
        case .failed?: 1
        case .done?, .changed?: 2
        case nil: session.isWorking ? 3 : 4
        }
    }
}

private struct DosiShelfRow<Item: Identifiable, Chip: View, More: View, Trailing: View>: View {
    let items: [Item]
    let widths: [CGFloat]
    let estimate: CGFloat
    @ViewBuilder let chip: (Item, Int, CGFloat) -> Chip
    @ViewBuilder let more: ([Item], Int) -> More
    @ViewBuilder let trailing: () -> Trailing
    @State private var room: CGFloat = 0

    var body: some View {
        let shown = DosiShelfFit.shown(widths, in: room > 0 ? room : estimate)
        HStack(spacing: 4) {
            HStack(spacing: DosiShelfFit.spacing) {
                ForEach(Array(items.prefix(shown).enumerated()), id: \.element.id) { order, item in
                    chip(item, order, widths[order])
                }
                if shown < items.count {
                    more(Array(items.dropFirst(shown)), shown)
                }
            }
            .padding(2)
            .frame(maxWidth: .infinity, alignment: .leading)
            .onGeometryChange(for: CGFloat.self) { $0.size.width - 4 } action: { room = $0 }
            .arrive(delay: 0.1, rise: 0)
            trailing()
        }
        .frame(height: 30)
    }
}

private struct DosiChip<Mark: View>: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let name: String
    let width: CGFloat
    let selected: Bool
    let shown: Bool
    let mark: Mark
    let sign: AnyView?

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
        HStack(spacing: 6) {
            mark
            Text(name).font(.system(size: 11.5, weight: selected ? .semibold : .medium))
                .foregroundStyle(shown || selected ? theme.colors.ink : theme.colors.inkMuted)
                .lineLimit(1).truncationMode(.middle).frame(maxWidth: DosiShelfFit.nameWidth, alignment: .leading)
            if let sign {
                sign.transition(AnyTransition.pop)
            }
        }
        .padding(.leading, 3).padding(.trailing, 8).frame(width: width, height: 26, alignment: .leading)
        .background {
            BezelBackground(shape: shape, fill: selected ? theme.colors.accentSoft : theme.colors.raised.mix(with: theme.colors.ink, by: hovered ? 0.05 : 0),
                            bezel: .quiet)
        }
        .contentShape(shape)
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .animation(theme.motion.snappy, value: selected)
    }
}

private struct DosiTerminalChip: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let order: Int
    let width: CGFloat
    @State private var glints = 0
    @State private var glintDelay = 0.0
    @State private var rings = 0

    private var selected: Bool {
        deps.workbench.section == .sessions && deps.workbench.selectedSessionId == session.id
    }

    private var waiting: Bool {
        [StatusSign.Form.hand, .question, .key].contains(deps.sign(of: session))
    }

    var body: some View {
        Button { deps.activateSession(session.id) } label: {
            DosiChip(
                name: session.name, width: width, selected: selected, shown: deps.workbench.stagedSessionIds.contains(session.id),
                mark: AgentMark(session: session, size: 20, activity: deps.activity(of: session)).fold(.terminal(session.id), at: .slot),
                sign: deps.sign(of: session).map { AnyView(StatusSign(form: $0, size: 9)) },
            )
            .glint(trigger: glints, delay: glintDelay, radius: Radius.sm)
            .ring(trigger: rings, radius: Radius.sm)
        }
        .buttonStyle(PressScaleStyle(scale: 0.95))
        .tip(verbatim: session.kind == .remote ? "\(session.name) · \(session.hostLabel)" : session.name)
        .accessibilityLabel(session.name)
        .accessibilityIdentifier("dosi.marks.terminal.\(AccessibilityIdentifier.token(session.id))")
        .accessibilityAddTraits(selected ? .isSelected : [])
        .onChange(of: deps.dosi.reading?.id) { _, id in
            guard id != nil, case let .tool(tool)? = deps.dosi.reading?.body, let target = tool.target else { return }
            switch target.kind {
            case .terminals:
                glintDelay = Double(min(order, 10)) * 0.07
                glints += 1
            case .terminal where deps.terminal(named: target.value)?.id == session.id:
                glintDelay = 0
                glints += 1
            default:
                break
            }
        }
        .onChange(of: waiting) { _, now in
            if now {
                rings += 1
            }
        }
        .animation(theme.motion.snappy, value: deps.sign(of: session))
    }
}

private struct DosiRoomChip: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    let mission: MissionEntry
    let order: Int
    let width: CGFloat
    @State private var glints = 0
    @State private var glintDelay = 0.0
    @State private var rings = 0

    private var selected: Bool {
        deps.workbench.section == .missions && deps.workbench.selectedMissionId == mission.id
    }

    var body: some View {
        Button { deps.workbench.section = .missions; deps.openMission(mission) } label: {
            DosiChip(
                name: mission.name, width: width, selected: selected, shown: true,
                mark: RoomSigil(key: mission.id, size: 20).fold(.room(mission.id), at: .slot),
                sign: mission.unread ? AnyView(NewDot()) : nil,
            )
            .glint(trigger: glints, delay: glintDelay, radius: Radius.sm)
            .ring(trigger: rings, color: theme.colors.accent, radius: Radius.sm)
        }
        .buttonStyle(PressScaleStyle(scale: 0.95))
        .tip(verbatim: mission.name)
        .accessibilityLabel(mission.name)
        .accessibilityIdentifier("dosi.marks.room.\(AccessibilityIdentifier.token(mission.id))")
        .accessibilityAddTraits(selected ? .isSelected : [])
        .onChange(of: deps.dosi.reading?.id) { _, id in
            guard id != nil, case let .tool(tool)? = deps.dosi.reading?.body, let target = tool.target else { return }
            switch target.kind {
            case .rooms:
                glintDelay = Double(min(order, 10)) * 0.07
                glints += 1
            case .room where deps.room(named: target.value)?.id == mission.id:
                glintDelay = 0
                glints += 1
            default:
                break
            }
        }
        .onChange(of: mission.unread) { _, now in
            if now {
                rings += 1
            }
        }
        .animation(theme.motion.snappy, value: mission.unread)
    }
}

private struct DosiAddMark: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    let title: LocalizedStringKey
    let identifier: String
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Image(systemName: "plus").font(.system(size: 11, weight: .bold))
                .foregroundStyle(hovered ? theme.colors.accentStrong : theme.colors.inkFaint)
                .frame(width: 26, height: 26)
                .background {
                    RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
                        .strokeBorder(hovered ? theme.colors.accent : theme.colors.hairline.mix(with: theme.colors.accent, by: 0.25),
                                      style: StrokeStyle(lineWidth: 1.5, dash: [3, 2.5]))
                }
                .contentShape(Rectangle())
        }
        .buttonStyle(PressScaleStyle())
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .tip(title).accessibilityLabel(Text(title))
        .accessibilityIdentifier(identifier)
    }
}
