import Foundation
import KodosiBridge
import Observation

struct LauncherRequest: Equatable {
    var roomId: String?
}

struct LauncherDraft: Equatable {
    var prompt = ""
    var folder: String?
    var remoteFolders: [String: String] = [:]
}

@MainActor
@Observable
final class WorkbenchState {
    enum Section {
        case sessions, missions, people, settings
    }

    enum SettingsSection: String, CaseIterable { case terminal, providers, dosi, devices, general }

    static let maximumStagedSessions = 6
    static let sidebarWidths: ClosedRange<CGFloat> = 220 ... 380
    static let dosiWidths: ClosedRange<CGFloat> = 340 ... 640
    var section: Section = .sessions
    var settingsSection: SettingsSection = .terminal
    var selectedSessionId: String?
    var stagedSessionIds: [String] = []
    var focusedSessionId: String?
    var detailsSessionId: String?
    var sharingSessionId: String?
    var showsHistory = false
    var historyProvider: Provider = .claude
    var showsPalette = false
    var showsNewRoom = false
    var launcher: LauncherRequest?
    var launcherDraft = LauncherDraft()
    var launcherFocus = 0
    var selectedMissionId: String?
    var dismissedDeviceRequests: Set<String> = []
    var approvalNoticeDismissed = false
    var focusesDeviceCode = false
    var sidebarCollapsed = false {
        didSet {
            defaults.set(sidebarCollapsed, forKey: "sidebar.collapsed")
            if sidebarCollapsed != oldValue {
                fold()
            }
        }
    }

    var dosiOpen = false {
        didSet {
            defaults.set(dosiOpen, forKey: "dosi.open")
            if dosiOpen != oldValue {
                fold()
            }
        }
    }

    var dosiOn = true {
        didSet {
            defaults.set(dosiOn, forKey: "dosi.on")
            if !dosiOn {
                dosiOpen = false
            }
        }
    }

    private(set) var sidebarWidth: CGFloat = Metrics.sidebarWidth
    private(set) var dosiWidth: CGFloat = Metrics.dosiWidth
    private(set) var folding = false

    private struct Departure {
        let id: String
        let owner: String?
        let host: String?
        let place: Int
        let selected: Bool
        let focused: Bool
    }

    private let defaults: UserDefaults
    private var accountKey: String?
    private var restoredIds: [String] = []
    private var savedStage: [String] = []
    private var known: [String: RuntimeSession] = [:]
    private var departures: [Departure] = []
    private var closing: Set<String> = []
    @ObservationIgnored private var unfolding: Task<Void, Never>?

    init(defaults: UserDefaults) {
        self.defaults = defaults
        sidebarCollapsed = defaults.bool(forKey: "sidebar.collapsed")
        dosiOn = defaults.object(forKey: "dosi.on") as? Bool ?? true
        dosiOpen = dosiOn && defaults.bool(forKey: "dosi.open")
        if let width = defaults.object(forKey: "sidebar.width") as? Double {
            sidebarWidth = CGFloat(width).clamped(to: Self.sidebarWidths)
        }
        if let width = defaults.object(forKey: "dosi.width") as? Double {
            dosiWidth = CGFloat(width).clamped(to: Self.dosiWidths)
        }
    }

    var shownSidebarWidth: CGFloat {
        sidebarCollapsed ? Metrics.sidebarRailWidth : dosiOpen ? dosiWidth : sidebarWidth
    }

    var foldPlace: FoldPlace {
        sidebarCollapsed ? .rail : dosiOpen ? .slot : .row
    }

    func resizeSidebar(to width: CGFloat) {
        if dosiOpen {
            dosiWidth = width.clamped(to: Self.dosiWidths)
            defaults.set(Double(dosiWidth), forKey: "dosi.width")
        } else {
            sidebarWidth = width.clamped(to: Self.sidebarWidths)
            defaults.set(Double(sidebarWidth), forKey: "sidebar.width")
        }
    }

    func openDosi() {
        guard dosiOn else { return }
        sidebarCollapsed = false
        dosiOpen = true
    }

    func closeDosi() {
        dosiOpen = false
    }

    func toggleDosi() {
        if dosiOpen, !sidebarCollapsed {
            dosiOpen = false
        } else {
            openDosi()
        }
    }

    private func fold() {
        folding = true
        unfolding?.cancel()
        unfolding = Task { [weak self] in
            try? await Task.sleep(for: .milliseconds(760))
            guard !Task.isCancelled else { return }
            self?.folding = false
        }
    }

    func selectSession(_ id: String) {
        selectedSessionId = id
    }

    func resume(_ provider: Provider) {
        historyProvider = provider
        showsHistory = true
    }

    func showAccount(focusingCode: Bool = false) {
        settingsSection = .devices
        section = .settings
        focusesDeviceCode = focusingCode
    }

    func showSession(_ id: String, inRoom roomId: String? = nil) {
        guard stagedSessionIds.contains(id) || stagedSessionIds.count < Self.maximumStagedSessions else { return }
        section = roomId == nil ? .sessions : .missions
        if let roomId {
            selectedMissionId = roomId
        }
        selectedSessionId = id
        if !stagedSessionIds.contains(id) {
            stagedSessionIds.append(id)
        }
        if focusedSessionId != nil {
            focusedSessionId = id
        }
        save()
    }

    func dismissSession(_ id: String) {
        stagedSessionIds.removeAll { $0 == id }
        if selectedSessionId == id {
            selectedSessionId = stagedSessionIds.first
        }
        if focusedSessionId == id {
            focusedSessionId = nil
        }
        if detailsSessionId == id {
            detailsSessionId = nil
        }
        if sharingSessionId == id {
            sharingSessionId = nil
        }
        save()
    }

    func selectAdjacentSession(offset: Int) {
        guard !stagedSessionIds.isEmpty else { return }
        let index = selectedSessionId.flatMap { stagedSessionIds.firstIndex(of: $0) } ?? 0
        let next = stagedSessionIds[(index + offset + stagedSessionIds.count) % stagedSessionIds.count]
        selectedSessionId = next
        if focusedSessionId != nil {
            focusedSessionId = next
        }
    }

    func toggleFocus(_ id: String) {
        selectedSessionId = id
        focusedSessionId = focusedSessionId == id ? nil : id
    }

    func noteClosing(_ id: String) {
        closing.insert(id)
    }

    func cancelClosing(_ id: String) {
        closing.remove(id)
    }

    func switchAccount(_ userId: String?) {
        save()
        accountKey = "stage." + (userId.map { Data($0.utf8).base64EncodedString() } ?? "local")
        restoredIds = accountKey.flatMap { defaults.stringArray(forKey: $0) } ?? []
        savedStage = restoredIds
        known = [:]
        departures = []
        closing = []
        stagedSessionIds.removeAll()
        selectedSessionId = nil
        focusedSessionId = nil
        detailsSessionId = nil
        sharingSessionId = nil
        selectedMissionId = nil
        showsHistory = false
        showsPalette = false
        showsNewRoom = false
        launcher = nil
        launcherDraft = LauncherDraft()
        dismissedDeviceRequests = []
        approvalNoticeDismissed = false
        focusesDeviceCode = false
    }

    func reconcile(_ sessions: [RuntimeSession]) -> [String] {
        let current = Dictionary(sessions.map { ($0.id, $0) }, uniquingKeysWith: { first, _ in first })
        let restarted = Dictionary(
            sessions.filter { $0.kind == .local }.compactMap { session in session.restoredFrom.map { ($0, session.id) } },
            uniquingKeysWith: { first, _ in first },
        )
        restoredIds = restoredIds.map { restarted[$0] ?? $0 }
        for (place, id) in stagedSessionIds.enumerated().reversed() {
            if let session = current[id], restart(of: session.id, owner: session.ownerUserId, host: session.hostDeviceId, in: sessions) == nil {
                continue
            }
            if let session = current[id] ?? known[id] {
                depart(session, from: place)
            } else {
                dismissSession(id)
            }
        }
        known = current
        closing.formIntersection(current.keys)
        let followed = follow(sessions)
        let listed = Set(sessions.filter { !$0.hidden }.map(\.id))
        let restore = Array(restoredIds.filter { listed.contains($0) }.prefix(max(0, Self.maximumStagedSessions - stagedSessionIds.count)))
        restoredIds.removeAll { current[$0] != nil }
        let rank = { (id: String) in self.savedStage.firstIndex { (restarted[$0] ?? $0) == id } ?? -1 }
        for id in restore where !stagedSessionIds.contains(id) {
            let position = stagedSessionIds.firstIndex { rank($0) > rank(id) } ?? stagedSessionIds.endIndex
            stagedSessionIds.insert(id, at: position)
        }
        if selectedSessionId == nil, !restore.isEmpty {
            selectedSessionId = stagedSessionIds.first
        }
        if let id = sharingSessionId, current[id] == nil {
            sharingSessionId = nil
        }
        if let id = detailsSessionId, current[id] == nil {
            detailsSessionId = nil
        }
        save()
        return followed + restore
    }

    private func restart(of id: String, owner: String?, host: String?, in sessions: [RuntimeSession]) -> RuntimeSession? {
        sessions.first { $0.restoredFrom == id && $0.ownerUserId == owner && $0.hostDeviceId == host }
    }

    private func depart(_ session: RuntimeSession, from place: Int) {
        if closing.remove(session.id) == nil {
            departures.append(Departure(
                id: session.id, owner: session.ownerUserId, host: session.hostDeviceId, place: place,
                selected: selectedSessionId == session.id, focused: focusedSessionId == session.id,
            ))
            departures = Array(departures.suffix(64))
            if selectedSessionId == session.id {
                selectedSessionId = nil
            }
        }
        dismissSession(session.id)
    }

    private func follow(_ sessions: [RuntimeSession]) -> [String] {
        var followed: [String] = []
        for departure in departures.sorted(by: { $0.place < $1.place }) {
            guard let next = restart(of: departure.id, owner: departure.owner, host: departure.host, in: sessions) else { continue }
            departures.removeAll { $0.id == departure.id }
            guard !next.hidden, !stagedSessionIds.contains(next.id), stagedSessionIds.count < Self.maximumStagedSessions else { continue }
            stagedSessionIds.insert(next.id, at: min(departure.place, stagedSessionIds.count))
            if departure.selected, selectedSessionId == nil {
                selectedSessionId = next.id
            }
            if departure.focused, focusedSessionId == nil, selectedSessionId == next.id {
                focusedSessionId = next.id
            }
            followed.append(next.id)
        }
        return followed
    }

    private func save() {
        guard let accountKey else { return }
        let pending = restoredIds.filter { !stagedSessionIds.contains($0) }
        defaults.set(Array((stagedSessionIds + pending).prefix(64)), forKey: accountKey)
    }
}
