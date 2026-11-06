/// The order in which a person looked at terminals, so that the stage knows which one waited
/// longest.
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
    /// In single view another terminal can always come on the stage. When the stage is full, the
    /// hidden terminal that the person looked at longest ago leaves it and keeps running.
    func makingRoom(for id: String) -> String? {
        guard focusedSessionId != nil, !stagedSessionIds.contains(id), stagedSessionIds.count >= Self.maximumStagedSessions else {
            return nil
        }
        return looks.oldest(of: stagedSessionIds.filter { $0 != focusedSessionId })
    }
}
