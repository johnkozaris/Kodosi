import Foundation
import KodosiBridge
import Observation

struct LauncherRequest: Equatable {
    var folder: String?
    var roomId: String?
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
    var homeFolder: String?
    var launcherFocus = 0
    var selectedMissionId: String?
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

    private let defaults: UserDefaults
    private var accountKey: String?
    private var restoredIds: [String] = []
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

    func switchAccount(_ userId: String?) {
        save()
        accountKey = userId.map { "stage.\(Data($0.utf8).base64EncodedString())" }
        restoredIds = accountKey.flatMap { defaults.stringArray(forKey: $0) } ?? []
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
    }

    func reconcile(_ sessions: [RuntimeSession]) -> [String] {
        let available = Set(sessions.map(\.id))
        for id in stagedSessionIds where !available.contains(id) {
            dismissSession(id)
        }
        let restore = Array(restoredIds.filter { available.contains($0) }.prefix(max(0, Self.maximumStagedSessions - stagedSessionIds.count)))
        restoredIds.removeAll { available.contains($0) }
        for id in restore where !stagedSessionIds.contains(id) {
            stagedSessionIds.append(id)
        }
        if selectedSessionId == nil {
            selectedSessionId = stagedSessionIds.first
        }
        if let id = sharingSessionId, !available.contains(id) {
            sharingSessionId = nil
        }
        if let id = detailsSessionId, !available.contains(id) {
            detailsSessionId = nil
        }
        save()
        return restore
    }

    private func save() {
        guard let accountKey else { return }
        let pending = restoredIds.filter { !stagedSessionIds.contains($0) }
        defaults.set(Array((stagedSessionIds + pending).prefix(64)), forKey: accountKey)
    }
}
