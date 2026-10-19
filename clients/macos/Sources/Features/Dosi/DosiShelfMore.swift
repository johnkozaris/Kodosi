import KodosiBridge
import KodosiDesign
import SwiftUI

private struct DosiMoreChip<List: View>: View {
    @Environment(\.theme) private var theme
    @State private var hovered = false
    @State private var open = false
    let count: Int
    let needs: Bool
    let selected: Bool
    let label: Text
    let identifier: String
    let glints: Int
    let glintDelay: Double
    @ViewBuilder let list: () -> List

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
        Button { open.toggle() } label: {
            HStack(spacing: 4) {
                Text(verbatim: "\(count)").font(.system(size: 11.5, weight: .semibold).monospacedDigit())
                    .contentTransition(.numericText(value: Double(count)))
                Image(systemName: "chevron.down").font(.system(size: 8, weight: .bold))
                    .rotationEffect(.degrees(open ? 180 : 0))
            }
            .foregroundStyle(needs ? theme.colors.accentStrong : open || hovered ? theme.colors.ink : theme.colors.inkMuted)
            .frame(width: DosiShelfFit.more(count), height: 26)
            .background {
                BezelBackground(shape: shape, fill: open || selected ? theme.colors.accentSoft
                    : theme.colors.raised.mix(with: theme.colors.ink, by: hovered ? 0.05 : 0), bezel: .quiet)
            }
            .contentShape(shape)
            .glint(trigger: glints, delay: glintDelay, radius: Radius.sm)
            .offset(y: hovered ? -1 : 0)
        }
        .buttonStyle(PressScaleStyle(scale: 0.95))
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .animation(theme.motion.snappy, value: open)
        .animation(theme.motion.snappy, value: count)
        .help(label)
        .accessibilityLabel(label)
        .accessibilityIdentifier(identifier)
        .kodosiPopover(isPresented: $open, arrowEdge: .bottom, alignment: .leading) { list() }
    }
}

private struct DosiMoreList<Content: View>: View {
    let count: Int
    @ViewBuilder let content: Content

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 2) { content }.padding(6)
        }
        .frame(width: 240, height: min(CGFloat(count) * 30 + 10, 312))
    }
}

private struct DosiMoreRow<Mark: View, Sign: View>: View {
    @Environment(\.theme) private var theme
    @Environment(\.closePanel) private var close
    @State private var hovered = false
    let name: String
    let detail: String?
    let selected: Bool
    @ViewBuilder let mark: () -> Mark
    @ViewBuilder let sign: () -> Sign
    let action: () -> Void

    var body: some View {
        Button {
            close()
            action()
        } label: {
            HStack(spacing: 8) {
                mark()
                Text(name).font(.system(size: 12, weight: selected ? .semibold : .medium)).foregroundStyle(theme.colors.ink)
                    .lineLimit(1).truncationMode(.middle)
                Spacer(minLength: 6)
                if let detail {
                    Text(detail).font(.system(size: 11)).foregroundStyle(theme.colors.inkFaint).lineLimit(1)
                }
                sign()
            }
            .padding(.horizontal, 7).frame(height: 28)
            .background {
                RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
                    .fill(selected ? theme.colors.accentSoft : hovered ? theme.colors.raised.mix(with: theme.colors.ink, by: 0.06) : .clear)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .accessibilityLabel(name)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

struct DosiMoreTerminals: View {
    @Environment(AppDependencies.self) private var deps
    let sessions: [RuntimeSession]
    let order: Int
    let needs: Bool
    @State private var glints = 0
    @State private var glintDelay = 0.0

    var body: some View {
        DosiMoreChip(
            count: sessions.count, needs: needs, selected: sessions.contains { selected($0) },
            label: Text("\(sessions.count) more terminals"), identifier: "dosi.marks.more", glints: glints, glintDelay: glintDelay
        ) {
            DosiMoreList(count: sessions.count) {
                ForEach(Array(sessions.enumerated()), id: \.element.id) { index, session in
                    DosiMoreRow(name: session.name, detail: session.kind == .remote ? session.hostLabel : nil, selected: selected(session)) {
                        AgentMark(session: session, size: 18, activity: deps.activity(of: session))
                    } sign: {
                        if let form = deps.sign(of: session) {
                            StatusSign(form: form, size: 9)
                        }
                    } action: {
                        deps.activateSession(session.id)
                    }
                    .arrive(delay: Double(min(index, 10)) * 0.025, rise: 6)
                    .accessibilityIdentifier("dosi.marks.more.\(AccessibilityIdentifier.token(session.id))")
                }
            }
        }
        .background {
            ZStack {
                ForEach(sessions) { session in
                    Color.clear.frame(width: 20, height: 20).fold(.terminal(session.id), at: .slot)
                }
            }
        }
        .onChange(of: deps.dosi.reading?.id) { _, id in
            guard id != nil, case let .tool(tool)? = deps.dosi.reading?.body, let target = tool.target else { return }
            switch target.kind {
            case .terminals:
                glintDelay = Double(min(order, 10)) * 0.12
                glints += 1
            case .terminal where sessions.contains(where: { $0.id == deps.terminal(named: target.value)?.id }):
                glintDelay = 0
                glints += 1
            default:
                break
            }
        }
    }

    private func selected(_ session: RuntimeSession) -> Bool {
        deps.workbench.section == .sessions && deps.workbench.selectedSessionId == session.id
    }
}

struct DosiMoreRooms: View {
    @Environment(AppDependencies.self) private var deps
    let missions: [MissionEntry]
    let order: Int
    @State private var glints = 0
    @State private var glintDelay = 0.0

    var body: some View {
        DosiMoreChip(
            count: missions.count, needs: missions.contains(where: \.unread), selected: missions.contains { selected($0) },
            label: Text("\(missions.count) more rooms"), identifier: "dosi.marks.room.more", glints: glints, glintDelay: glintDelay
        ) {
            DosiMoreList(count: missions.count) {
                ForEach(Array(missions.enumerated()), id: \.element.id) { index, mission in
                    DosiMoreRow(name: mission.name, detail: nil, selected: selected(mission)) {
                        RoomSigil(key: mission.id, size: 18)
                    } sign: {
                        if mission.unread {
                            NewDot()
                        }
                    } action: {
                        deps.workbench.section = .missions
                        deps.openMission(mission)
                    }
                    .arrive(delay: Double(min(index, 10)) * 0.025, rise: 6)
                    .accessibilityIdentifier("dosi.marks.room.more.\(AccessibilityIdentifier.token(mission.id))")
                }
            }
        }
        .background {
            ZStack {
                ForEach(missions) { mission in
                    Color.clear.frame(width: 20, height: 20).fold(.room(mission.id), at: .slot)
                }
            }
        }
        .onChange(of: deps.dosi.reading?.id) { _, id in
            guard id != nil, case let .tool(tool)? = deps.dosi.reading?.body, let target = tool.target else { return }
            switch target.kind {
            case .rooms:
                glintDelay = Double(min(order, 10)) * 0.12
                glints += 1
            case .room where missions.contains(where: { $0.id == deps.room(named: target.value)?.id }):
                glintDelay = 0
                glints += 1
            default:
                break
            }
        }
    }

    private func selected(_ mission: MissionEntry) -> Bool {
        deps.workbench.section == .missions && deps.workbench.selectedMissionId == mission.id
    }
}
