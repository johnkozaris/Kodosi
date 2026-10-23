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
    nonisolated static let space = "sidebar.fold"
    var frames: [String: CGRect] = [:]
    var tucked: Set<String> = []
    var views: [FoldPlace: CGRect] = [:]

    func spot(_ mark: FoldMark, at place: FoldPlace) -> (rect: CGRect, shown: Bool)? {
        let id = mark.id(place)
        guard let rect = frames[id] else { return nil }
        if tucked.contains(id) {
            return (CGRect(x: rect.midX - rect.width * 0.2, y: rect.midY - rect.height * 0.2, width: rect.width * 0.4,
                           height: rect.height * 0.4), false)
        }
        guard let view = views[place], rect.midY < view.minY || rect.midY > view.maxY else { return (rect, true) }
        let y = min(max(rect.minY, view.minY), view.maxY - rect.height)
        return (CGRect(x: rect.minX, y: y, width: rect.width, height: rect.height), false)
    }
}

extension EnvironmentValues {
    @Entry var sidebarFold: Namespace.ID?
    @Entry var sidebarFolding = false
    @Entry var sidebarFrames: FoldFrames?
    @Entry var sidebarPointer: SidebarPointer?
}

@MainActor
@Observable
final class SidebarPointer {
    var location: CGPoint?
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
    let tucked: Bool

    func body(content: Content) -> some View {
        content
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .named(FoldFrames.space)) } action: { rect in
                frames?.frames[id] = rect
                if tucked {
                    frames?.tucked.insert(id)
                } else {
                    frames?.tucked.remove(id)
                }
            }
            .opacity(folding ? 0 : 1)
            .animation(nil, value: folding)
    }
}

extension View {
    func fold(_ id: String) -> some View {
        modifier(FoldSource(id: id))
    }

    func fold(_ mark: FoldMark, at place: FoldPlace, tucked: Bool = false) -> some View {
        modifier(FoldMarkPlace(id: mark.id(place), tucked: tucked))
    }

    func leaving(_ theme: AppTheme) -> some View {
        transition(.asymmetric(insertion: .identity, removal: theme.motion.reduced ? .identity : .opacity.animation(theme.motion.hover)))
    }
}

private struct Flight: Equatable {
    var rect: CGRect
    var shown: Bool
    let size: CGFloat
}

struct FoldFlights: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.sidebarFrames) private var frames
    let place: FoldPlace
    @State private var flights: [FoldMark: Flight] = [:]
    @State private var lanes: [FoldMark: CGFloat] = [:]
    @State private var flight = 0

    var body: some View {
        ZStack(alignment: .topLeading) {
            ForEach(deps.listedSessions) { session in
                if let flight = flights[.terminal(session.id)] {
                    copy(AgentMark(session: session, size: flight.size, activity: deps.activity(of: session)), flight,
                         lane: lanes[.terminal(session.id)])
                }
            }
            ForEach(deps.missions) { mission in
                if let flight = flights[.room(mission.id)] {
                    copy(RoomSigil(key: mission.id, size: flight.size), flight, lane: lanes[.room(mission.id)])
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .allowsHitTesting(false)
        .accessibilityHidden(true)
        .onChange(of: place) { old, new in fly(from: old, to: new) }
        .onChange(of: deps.workbench.folding) { _, now in
            if !now {
                land()
            }
        }
    }

    private func copy(_ view: some View, _ flight: Flight, lane: CGFloat?) -> some View {
        view.scaleEffect(flight.rect.width / flight.size, anchor: .topLeading)
            .frame(width: flight.rect.width, height: flight.rect.height, alignment: .topLeading)
            .opacity(flight.shown ? 1 : 0)
            .offset(y: flight.rect.minY)
            .offset(x: lane ?? flight.rect.minX)
    }

    private func land() {
        flight += 1
        var still = Transaction(animation: nil)
        still.disablesAnimations = true
        withTransaction(still) {
            flights = [:]
            lanes = [:]
        }
    }

    private func fly(from old: FoldPlace, to new: FoldPlace) {
        guard let frames, !theme.motion.reduced else { return }
        flight += 1
        let current = flight
        let marks = deps.listedSessions.map { FoldMark.terminal($0.id) } + deps.missions.map { FoldMark.room($0.id) }
        var start = flights
        var starts = lanes
        var fresh: Set<FoldMark> = []
        for mark in marks where start[mark] == nil {
            guard let from = frames.spot(mark, at: old) else { continue }
            let size = (frames.spot(mark, at: new).flatMap { $0.shown ? $0.rect.width : nil } ?? from.rect.width).rounded()
            start[mark] = Flight(rect: from.rect, shown: from.shown, size: max(size, 1))
            starts[mark] = from.rect.minX
            fresh.insert(mark)
        }
        var still = Transaction(animation: nil)
        still.disablesAnimations = true
        withTransaction(still) {
            flights = start
            lanes = starts
        }
        Task { @MainActor in
            var aimed: [FoldMark: CGRect] = [:]
            let last = Double(min(marks.count - 1, 6))
            for wait in [17, 50, 120] {
                try? await Task.sleep(for: .milliseconds(wait))
                guard current == flight else { return }
                for (index, mark) in marks.enumerated() where flights[mark] != nil {
                    guard let end = frames.spot(mark, at: new) else {
                        if wait == 120, aimed[mark] == nil {
                            withAnimation(theme.motion.fade) { flights[mark]?.shown = false }
                        }
                        continue
                    }
                    if let aim = aimed[mark], abs(aim.minX - end.rect.minX) < 0.5, abs(aim.minY - end.rect.minY) < 0.5,
                       abs(aim.width - end.rect.width) < 0.5
                    {
                        continue
                    }
                    let staggered = aimed[mark] == nil && fresh.contains(mark)
                    aimed[mark] = end.rect
                    let step = Double(min(index, 6))
                    let delay = staggered ? (new == .row ? last - step : step) * 0.035 : 0
                    withAnimation(theme.motion.spring?.delay(delay)) {
                        flights[mark]?.rect = end.rect
                        flights[mark]?.shown = end.shown
                    }
                    withAnimation(theme.motion.soft?.delay(delay)) { lanes[mark] = end.rect.minX }
                }
            }
        }
    }
}
