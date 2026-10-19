import AppKit
import KodosiBridge
import KodosiDesign
import SwiftUI

struct SidebarDosiRow: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var hovered = false
    @State private var glints = 0
    @State private var rings = 0

    private var line: String? {
        switch deps.dosi.state {
        case .unavailable: deps.dosiStatus
        case .working, .stopping: deps.dosiDeed ?? deps.dosi.mood().label.lowercased()
        case .idle, .connecting: deps.dosi.openQuestions.isEmpty ? nil : String(localized: "waits for you")
        }
    }

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
        Button { deps.workbench.openDosi() } label: {
            HStack(spacing: 10) {
                DosiFace(mood: deps.dosi.mood(), size: 24).ring(trigger: rings, color: theme.colors.accent, radius: 7.2)
                    .fold(FoldPart.face)
                HStack(spacing: 10) {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(verbatim: "Dosi").appTextStyle(.body).fontWeight(.semibold).foregroundStyle(theme.colors.ink)
                        if let line {
                            Text(line).font(.system(size: 10.5, design: .monospaced)).foregroundStyle(theme.colors.inkFaint)
                                .lineLimit(1).shimmer(deps.dosi.busy)
                                .contentTransition(.interpolate).transition(.opacity.combined(with: .offset(y: 3)))
                        }
                    }
                    Spacer(minLength: 4)
                    if !deps.dosi.openQuestions.isEmpty {
                        StatusSign(form: .question).transition(AnyTransition.pop)
                    } else if deps.dosi.unseen {
                        NewDot().transition(AnyTransition.pop)
                    }
                    Text(verbatim: "⌘J").appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint.opacity(hovered ? 1 : 0.7))
                }
                .arrive(delay: 0.1, rise: 0)
            }
            .padding(.leading, 8).padding(.trailing, 10).frame(height: 40)
            .background {
                shape.fill(theme.colors.ink.opacity(hovered ? 0.075 : 0.045)).fold(FoldPart.box)
            }
            .glint(trigger: glints, radius: Radius.md)
            .contentShape(shape)
        }
        .buttonStyle(PressScaleStyle(scale: 0.98))
        .onChange(of: deps.dosi.unseen) { _, now in
            if now {
                glints += 1
            }
        }
        .onChange(of: deps.dosi.openQuestions.count) { old, new in
            if new > old {
                rings += 1
            }
        }
        .onHover { hovered = $0 }
        .animation(theme.motion.hover, value: hovered)
        .animation(theme.motion.snappy, value: line)
        .animation(theme.motion.snappy, value: deps.dosi.openQuestions.map(\.id))
        .animation(theme.motion.snappy, value: deps.dosi.unseen)
        .help("Dosi")
        .accessibilityIdentifier("sidebar.dosi")
    }
}

struct RailDosiIsland: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var glints = 0
    @State private var rings = 0

    private var waiting: Bool {
        !deps.dosi.openQuestions.isEmpty || deps.dosi.unseen
    }

    var body: some View {
        Button { deps.workbench.openDosi() } label: {
            HStack(spacing: 6) {
                DosiFace(mood: deps.dosi.mood(), size: 24).ring(trigger: rings, color: theme.colors.accent, radius: 7.2)
                    .fold(FoldPart.face)
                if !deps.dosi.openQuestions.isEmpty {
                    StatusSign(form: .question, size: 10).transition(AnyTransition.pop)
                } else if deps.dosi.unseen {
                    NewDot().transition(AnyTransition.pop)
                }
            }
            .padding(.horizontal, 8).frame(minWidth: 40, minHeight: 40)
            .background {
                RaisedBackground(shape: RoundedRectangle(cornerRadius: 12, style: .continuous), fill: theme.colors.raised,
                                 elevation: .resting)
                    .fold(FoldPart.box)
            }
            .glint(trigger: glints, radius: 12)
        }
        .buttonStyle(PressScaleStyle())
        .onChange(of: deps.dosi.unseen) { _, now in
            if now {
                glints += 1
            }
        }
        .onChange(of: deps.dosi.openQuestions.count) { old, new in
            if new > old {
                rings += 1
            }
        }
        .help(deps.dosiStatus)
        .accessibilityLabel(Text(verbatim: "Dosi")).accessibilityValue(Text(deps.dosiStatus))
        .accessibilityIdentifier("sidebar.dosi")
        .animation(theme.motion.snappy, value: waiting)
    }
}

struct DosiNewChatButton: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var asking = false

    var body: some View {
        IconButton(title: "New chat", symbol: "square.and.pencil", identifier: "dosi.new", active: asking) { asking = true }
            .disabled(deps.dosi.state == .unavailable)
            .kodosiPopover(isPresented: $asking, arrowEdge: .top) {
                VStack(alignment: .leading, spacing: 10) {
                    Text("Start a new chat?").appTextStyle(.headline).foregroundStyle(theme.colors.ink)
                    Text("Dosi starts again with no history, here and on your other computers.")
                        .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).fixedSize(horizontal: false, vertical: true)
                    HStack(spacing: 6) {
                        Spacer()
                        Button("Cancel") { asking = false }.buttonStyle(.kodosi(.ghost, size: .small))
                        Button("Start") {
                            asking = false
                            deps.renewDosi()
                        }
                        .buttonStyle(.kodosi(.primary, size: .small))
                        .keyboardShortcut(.defaultAction)
                        .accessibilityIdentifier("dosi.new.start")
                    }
                }
                .padding(14).frame(width: 264)
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
                    .onEnded { _ in start = nil }
            )
            .accessibilityHidden(true)
    }
}
