import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

struct SidebarDosiRow: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var hovered = false
    @State private var reacts = 0
    @State private var rings = 0

    var body: some View {
        let line = deps.dosiLine
        let calls = !deps.dosi.openQuestions.isEmpty || deps.dosi.unseen
        VStack(alignment: .leading, spacing: 0) {
            Rectangle().fill(theme.colors.hairline.mix(with: theme.colors.ground, by: 0.4)).frame(height: 1)
                .padding(.horizontal, 8).padding(.bottom, 6)
            Button { deps.workbench.openDosi() } label: {
                HStack(spacing: 9) {
                    LookingDosiFace(mood: deps.dosi.mood(), follows: !deps.dosi.busy, reacts: reacts)
                        .ring(trigger: rings, color: theme.colors.accent, radius: 6)
                        .fold(FoldPart.face)
                    ZStack(alignment: .leading) {
                        Text(line).id(line).transition(AnyTransition.roll)
                    }
                    .clipped()
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(calls || hovered ? theme.colors.ink : theme.colors.inkMuted)
                    .lineLimit(1).truncationMode(.tail)
                    .arrive(delay: 0.1, rise: 0)
                    Spacer(minLength: 4)
                    DosiTouched()
                    DosiSign()
                }
                .padding(.leading, 7).padding(.trailing, 10).frame(height: 32)
                .background { Color.clear.fold(FoldPart.box) }
                .hoverRow()
            }
            .buttonStyle(PressScaleStyle(scale: 0.98))
            .accessibilityIdentifier("sidebar.dosi")
        }
        .dosiCues(deps.dosi, reacts: $reacts, rings: $rings)
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .animation(theme.motion.snappy, value: line)
    }
}

struct RailDosiIsland: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var reacts = 0
    @State private var rings = 0

    var body: some View {
        Button { deps.workbench.openDosi() } label: {
            LookingDosiFace(mood: deps.dosi.mood(), follows: !deps.dosi.busy, reacts: reacts)
                .ring(trigger: rings, color: theme.colors.accent, radius: 6)
                .fold(FoldPart.face)
                .overlay(alignment: .topTrailing) {
                    DosiSign(size: 9).offset(x: 7, y: -7)
                }
                .frame(width: 40, height: 40)
                .background { Color.clear.fold(FoldPart.box) }
                .hoverRow(radius: Radius.lg)
        }
        .buttonStyle(PressScaleStyle())
        .dosiCues(deps.dosi, reacts: $reacts, rings: $rings)
        .tip(verbatim: deps.dosiLine)
        .accessibilityLabel(Text(verbatim: "Dosi")).accessibilityValue(Text(deps.dosiLine))
        .accessibilityIdentifier("sidebar.dosi")
    }
}

struct LookingDosiFace: View {
    @Environment(\.sidebarPointer) private var pointer
    let mood: DosiFace.Mood
    var size: CGFloat = 20
    var follows = true
    var reacts = 0
    @State private var center: CGPoint?

    var body: some View {
        DosiFace(mood: mood, size: size, look: look, reacts: reacts)
            .onGeometryChange(for: CGPoint.self) { proxy in
                let frame = proxy.frame(in: .named(FoldFrames.space))
                return CGPoint(x: frame.midX, y: frame.midY)
            } action: { center = $0 }
    }

    private var look: CGPoint {
        guard follows, let center, let location = pointer?.location else { return .zero }
        let dx = location.x - center.x
        let dy = location.y - center.y
        let distance = hypot(dx, dy)
        guard distance > size * 0.6 else { return .zero }
        let strength = min(1, distance / 200)
        let step = { (value: CGFloat) in (value * 4).rounded() / 4 }
        return CGPoint(x: step(dx / distance * strength), y: step(dy / distance * strength))
    }
}

private struct DosiTouched: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme

    var body: some View {
        let touched = deps.dosi.touched
        HStack(spacing: -3) {
            ForEach(touched, id: \.key) { target in
                Group {
                    if target.kind == .terminal, let session = deps.terminal(named: target.value) {
                        AgentMark(session: session, size: 16)
                    } else if let room = deps.room(named: target.value) {
                        RoomSigil(key: room.id, size: 16)
                    }
                }
                .transition(AnyTransition.pop)
            }
        }
        .animation(theme.motion.snappy, value: touched.map(\.key))
    }
}

private struct DosiSign: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    var size: CGFloat = 10

    var body: some View {
        ZStack {
            if !deps.dosi.openQuestions.isEmpty {
                StatusSign(form: .question, size: size).transition(AnyTransition.pop)
            } else if deps.dosi.unseen {
                NewDot().transition(AnyTransition.pop)
            }
        }
        .animation(theme.motion.snappy, value: deps.dosi.openQuestions.isEmpty)
        .animation(theme.motion.snappy, value: deps.dosi.unseen)
    }
}

struct DosiBackButton: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var hovered = false

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
        Button { deps.workbench.closeDosi() } label: {
            Image(systemName: "chevron.left").font(.system(size: 11, weight: .bold))
                .foregroundStyle(hovered ? theme.colors.ink : theme.colors.inkMuted)
                .offset(x: -0.5)
                .frame(width: 26, height: 26)
                .background { BezelBackground(shape: shape, fill: theme.buttonFace(lit: hovered), bezel: .quiet) }
                .contentShape(shape)
        }
        .buttonStyle(PressScaleStyle())
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .tip("Back to terminals and rooms")
        .accessibilityLabel(Text("Back to terminals and rooms"))
        .accessibilityIdentifier("dosi.close")
    }
}

struct DosiNewChatButton: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var asking = false

    var body: some View {
        IconButton(title: "New chat", symbol: "square.and.pencil", identifier: "dosi.new", open: asking) { asking = true }
            .disabled(deps.dosi.state == .unavailable)
            .kodosiPopover(isPresented: $asking, arrowEdge: .bottom, alignment: .trailing) {
                VStack(alignment: .leading, spacing: 10) {
                    Text("Start a new chat?").appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                    Text("Dosi forgets this chat, here and on your other computers.")
                        .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).fixedSize(horizontal: false, vertical: true)
                    HStack(spacing: 6) {
                        Spacer()
                        Button("Cancel") { asking = false }
                            .buttonStyle(.kodosi(.secondary, size: .small))
                            .keyboardShortcut(.defaultAction)
                        Button("Start new chat") {
                            asking = false
                            deps.renewDosi()
                        }
                        .buttonStyle(.kodosi(.danger, size: .small))
                        .accessibilityIdentifier("dosi.new.start")
                    }
                }
                .padding(14).frame(width: 280)
            }
    }
}

private struct DosiCue: Equatable {
    let unseen: Bool
    let questions: Int
}

private extension View {
    func dosiCues(_ dosi: DosiStore, reacts: Binding<Int>, rings: Binding<Int>) -> some View {
        onChange(of: DosiCue(unseen: dosi.unseen, questions: dosi.openQuestions.count)) { old, new in
            if new.questions > old.questions {
                rings.wrappedValue += 1
            } else if new.unseen, !old.unseen, new.questions == 0 {
                reacts.wrappedValue += 1
            }
        }
    }
}

struct SidebarResizeHandle: View {
    @Environment(AppDependencies.self) private var deps
    @State private var start: CGFloat?

    var body: some View {
        Color.clear
            .frame(width: 8)
            .contentShape(Rectangle())
            .onHover { inside in
                if inside {
                    NSCursor.columnResize.push()
                } else {
                    NSCursor.pop()
                }
            }
            .gesture(
                DragGesture(minimumDistance: 1, coordinateSpace: .global)
                    .onChanged { drag in
                        let origin = start ?? deps.workbench.shownSidebarWidth
                        start = origin
                        deps.workbench.resizeSidebar(to: origin + drag.translation.width)
                    }
                    .onEnded { _ in start = nil },
            )
            .accessibilityHidden(true)
    }
}
