import KodosiBridge
import KodosiDesign
import SwiftUI

enum FoldPlace: String {
    case row, slot, rail
}

enum FoldMark: Hashable {
    case terminal(String)
    case room(String)

    func id(_ place: FoldPlace) -> String {
        switch self {
        case let .terminal(id): "terminal.\(id).\(place.rawValue)"
        case let .room(id): "room.\(id).\(place.rawValue)"
        }
    }
}

enum FoldPart {
    static let box = "dosi.box"
    static let face = "dosi.face"
    static let newTerminal = "terminal.new"
}

@MainActor
final class FoldFrames {
    static let space = "sidebar.fold"
    var frames: [String: CGRect] = [:]
}

extension EnvironmentValues {
    @Entry var sidebarFold: Namespace.ID?
    @Entry var sidebarFolding = false
    @Entry var sidebarFrames: FoldFrames?
}

private struct FoldSource: ViewModifier {
    @Environment(\.sidebarFold) private var fold
    let id: String

    func body(content: Content) -> some View {
        if let fold {
            content.matchedGeometryEffect(id: id, in: fold)
        } else {
            content
        }
    }
}

private struct FoldMarkPlace: ViewModifier {
    @Environment(\.sidebarFrames) private var frames
    @Environment(\.sidebarFolding) private var folding
    let id: String

    func body(content: Content) -> some View {
        content
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .named(FoldFrames.space)) } action: { frames?.frames[id] = $0 }
            .opacity(folding ? 0 : 1)
            .animation(nil, value: folding)
    }
}

extension View {
    func fold(_ id: String) -> some View {
        modifier(FoldSource(id: id))
    }

    func fold(_ mark: FoldMark, at place: FoldPlace) -> some View {
        modifier(FoldMarkPlace(id: mark.id(place)))
    }

    func leaving(_ theme: AppTheme) -> some View {
        transition(.asymmetric(insertion: .identity, removal: theme.motion.reduced ? .identity : .opacity.animation(.easeOut(duration: 0.12))))
    }
}

struct FoldFlights: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.sidebarFrames) private var frames
    let place: FoldPlace
    @State private var flights: [FoldMark: CGRect] = [:]
    @State private var flight = 0

    var body: some View {
        ZStack(alignment: .topLeading) {
            ForEach(deps.sessions) { session in
                if let rect = flights[.terminal(session.id)] {
                    mark(AgentMark(session: session, size: 28), in: rect)
                }
            }
            ForEach(deps.missions) { mission in
                if let rect = flights[.room(mission.id)] {
                    mark(RoomSigil(key: mission.id, size: 28), in: rect)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
        .onChange(of: place) { old, new in fly(from: old, to: new) }
    }

    private func mark(_ view: some View, in rect: CGRect) -> some View {
        view.scaleEffect(rect.width / 28, anchor: .topLeading)
            .frame(width: rect.width, height: rect.height, alignment: .topLeading)
            .offset(x: rect.minX, y: rect.minY)
    }

    private func fly(from old: FoldPlace, to new: FoldPlace) {
        guard let frames, !theme.motion.reduced else { return }
        flight += 1
        let current = flight
        let marks = deps.sessions.map { FoldMark.terminal($0.id) } + deps.missions.map { FoldMark.room($0.id) }
        var start: [FoldMark: CGRect] = [:]
        for mark in marks {
            start[mark] = frames.frames[mark.id(old)]
        }
        var still = Transaction(animation: nil)
        still.disablesAnimations = true
        withTransaction(still) { flights = start }
        Task { @MainActor in
            try? await Task.sleep(for: .milliseconds(17))
            guard current == flight else { return }
            let last = Double(min(marks.count - 1, 7))
            for (index, mark) in marks.enumerated() {
                guard flights[mark] != nil, let end = frames.frames[mark.id(new)] else {
                    flights[mark] = nil
                    continue
                }
                let step = Double(min(index, 7))
                withAnimation(theme.motion.soft?.delay((new == .row ? last - step : step) * 0.04)) { flights[mark] = end }
            }
            try? await Task.sleep(for: .milliseconds(900))
            guard current == flight else { return }
            withTransaction(still) { flights = [:] }
        }
    }
}
