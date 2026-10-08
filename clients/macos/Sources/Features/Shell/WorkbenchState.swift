import Foundation
import Observation

@MainActor
@Observable
final class WorkbenchState {
    enum Section: String, CaseIterable, Identifiable {
        case sessions, missions, people, settings
        var id: String {
            rawValue
        }

        var label: String {
            switch self {
            case .sessions: String(localized: "Terminals")
            case .missions: String(localized: "Rooms")
            case .people: String(localized: "People")
            case .settings: String(localized: "Settings")
            }
        }

        var symbol: String {
            switch self {
            case .sessions: "terminal"
            case .missions: "square.stack"
            case .people: "person.2"
            case .settings: "gearshape"
            }
        }
    }

    enum SettingsSection { case terminal, providers, devices }

    static let maximumStagedSessions = 6
    var section: Section = .sessions
    var settingsSection: SettingsSection = .terminal
    var selectedSessionId: String?
    var stagedSessionIds: [String] = []
    var focusedSessionId: String?
    var detailsSessionId: String?
    var sharingSessionId: String?
    var showsHistory = false
    var selectedMissionId: String?
    var sidebarCollapsed = false {
        didSet { defaults.set(sidebarCollapsed, forKey: "sidebar.collapsed") }
    }

    let sidebarWidth: CGFloat = 230
    private let defaults: UserDefaults
    private var accountKey: String?
    private var restoredIds: [String] = []

    init(defaults: UserDefaults) {
        self.defaults = defaults
        sidebarCollapsed = defaults.bool(forKey: "sidebar.collapsed")
    }

    func selectSession(_ id: String) {
        selectedSessionId = id
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
    }

    func reconcile(_ sessions: [RuntimeSession]) -> [String] {
        let available = Set(sessions.map(\.id))
        let unreachable = Set(sessions.filter { $0.kind == .remote && $0.status == .reconnecting }.map(\.id))
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
        return restore.filter { !unreachable.contains($0) }
    }

    private func save() {
        guard let accountKey else { return }
        let pending = restoredIds.filter { !stagedSessionIds.contains($0) }
        defaults.set(Array((stagedSessionIds + pending).prefix(64)), forKey: accountKey)
    }
}
