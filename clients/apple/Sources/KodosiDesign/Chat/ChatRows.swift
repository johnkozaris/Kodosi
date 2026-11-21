import KodosiBridge

/// The chat as a person reads it: the work of a turn folds into one row before its answer.
struct ChatRow: Identifiable, Equatable {
    enum Kind: Equatable {
        case person(text: String, by: ChatPerson?)
        case answer(text: String, live: Bool)
        /// The calls of a turn and what the agent wrote between them, while `live` as far as they
        /// came.
        case work([ChatItem], live: Bool)
        case end(TurnEnd, ms: UInt64)
        case note(ChatNote)
    }

    static let longTurn: UInt64 = 30000

    let id: String
    let kind: Kind

    /// `live`: the agent works on the last turn.
    static func rows(_ items: [ChatItem], live: Bool) -> [ChatRow] {
        var rows: [ChatRow] = []
        var turn: [ChatItem] = []
        func fold(ended: Bool) {
            defer { turn = [] }
            guard !turn.isEmpty else { return }
            let answer: Int? = if ended {
                turn.lastIndex {
                    if case .text = $0.body {
                        true
                    } else {
                        false
                    }
                }
            } else if case .text = turn.last?.body {
                turn.count - 1
            } else {
                nil
            }
            var work = turn
            if let answer {
                work.remove(at: answer)
            }
            if work.tools.isEmpty {
                for item in work {
                    if case let .text(text, _) = item.body {
                        rows.append(ChatRow(id: item.id, kind: .answer(text: text, live: false)))
                    }
                }
            } else {
                rows.append(ChatRow(id: "work.\(work[0].id)", kind: .work(work, live: !ended)))
            }
            if let answer, case let .text(text, writing) = turn[answer].body {
                rows.append(ChatRow(id: turn[answer].id, kind: .answer(text: text, live: writing && !ended)))
            }
        }
        for item in items {
            switch item.body {
            case let .person(text, by):
                fold(ended: true)
                rows.append(ChatRow(id: item.id, kind: .person(text: text, by: by)))
            case .text, .tool:
                turn.append(item)
            case let .turn(end, ms):
                fold(ended: true)
                if end == .stopped || end == .failed || end == .refused || (end == .done && ms >= longTurn) {
                    rows.append(ChatRow(id: item.id, kind: .end(end, ms: ms)))
                }
            case let .note(note):
                fold(ended: true)
                if note != .other {
                    rows.append(ChatRow(id: item.id, kind: .note(note)))
                }
            case .other:
                break
            }
        }
        fold(ended: !live)
        return rows
    }
}

extension [ChatItem] {
    var tools: [ChatTool] {
        compactMap { item in
            if case let .tool(tool) = item.body {
                return tool
            }
            return nil
        }
    }
}
