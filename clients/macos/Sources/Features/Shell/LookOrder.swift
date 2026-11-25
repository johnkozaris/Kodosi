struct LookOrder {
    private var looks: [String: Int] = [:]
    private var count = 0

    mutating func look(at id: String) {
        count += 1
        looks[id] = count
    }

    mutating func forget(_ id: String) {
        looks[id] = nil
    }

    func oldest(of ids: [String]) -> String? {
        ids.min { (looks[$0] ?? 0) < (looks[$1] ?? 0) }
    }
}

extension WorkbenchState {
    /// In single-terminal view, evicts the least-recently-viewed hidden terminal from a full stage
    /// (without closing it) so another terminal can always be opened.
    func makingRoom(for id: String) -> String? {
        guard focusedSessionId != nil, !stagedSessionIds.contains(id), stagedSessionIds.count >= Self.maximumStagedSessions else {
            return nil
        }
        return looks.oldest(of: stagedSessionIds.filter { $0 != focusedSessionId })
    }
}
