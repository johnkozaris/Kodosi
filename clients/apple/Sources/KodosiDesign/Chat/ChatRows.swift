import KodosiBridge

/// The chat as a person reads it: the agent's text during a turn shows as its own rows, and each
/// run of steps between two texts folds into a single row.
struct ChatRow: Identifiable, Equatable {
    enum Kind: Equatable {
        case person(text: String, by: ChatPerson?)
        case answer(text: String, live: Bool)
        case plan(text: String, live: Bool)
        /// The tool calls of a turn between two of the agent's texts, as far as they have arrived.
        case work([ChatItem])
        case end(TurnEnd, ms: UInt64)
        case note(ChatNote)
    }

    static let longTurn: UInt64 = 30000

    let id: String
    let kind: Kind
    /// Host Unix timestamp in milliseconds for the row's first item.
    let at: UInt64

    static func rows(_ items: [ChatItem], live: Bool) -> [ChatRow] {
        var rows: [ChatRow] = []
        var turn: [ChatItem] = []
        func fold(ended: Bool) {
            defer { turn = [] }
            guard !turn.isEmpty else { return }
            let answer: Int? = if ended {
                turn.lastIndex(where: \.answers)
            } else if turn.last?.answers == true {
                turn.count - 1
            } else {
                nil
            }
            var work: [ChatItem] = []
            func endWork() {
                if let first = work.first {
                    rows.append(ChatRow(id: "work.\(first.id)", kind: .work(work), at: first.at))
                }
                work = []
            }
            for (index, item) in turn.enumerated() {
                if index == answer {
                    endWork()
                    if let row = answerRow(item, ended: ended) {
                        rows.append(row)
                    }
                } else if case let .text(text, _) = item.body {
                    endWork()
                    rows.append(ChatRow(id: item.id, kind: .answer(text: text, live: false), at: item.at))
                } else {
                    work.append(item)
                }
            }
            endWork()
        }
        for item in items {
            switch item.body {
            case let .person(text, by):
                fold(ended: true)
                rows.append(ChatRow(id: item.id, kind: .person(text: text, by: by), at: item.at))
            case .text, .tool:
                turn.append(item)
            case let .turn(end, ms):
                fold(ended: true)
                if end == .stopped || end == .failed || end == .refused || (end == .done && ms >= longTurn) {
                    rows.append(ChatRow(id: item.id, kind: .end(end, ms: ms), at: item.at))
                }
            case let .note(note):
                fold(ended: true)
                rows.append(ChatRow(id: item.id, kind: .note(note), at: item.at))
            }
        }
        fold(ended: !live)
        return rows
    }

    private static func answerRow(_ item: ChatItem, ended: Bool) -> ChatRow? {
        switch item.body {
        case let .text(text, writing):
            ChatRow(id: item.id, kind: .answer(text: text, live: writing && !ended), at: item.at)
        case let .tool(tool):
            if case let .plan(text)? = tool.detail, !text.isEmpty {
                ChatRow(id: item.id, kind: .plan(text: text, live: tool.state.isOpen && !ended), at: item.at)
            } else {
                nil
            }
        default:
            nil
        }
    }
}

extension ChatItem {
    var answers: Bool {
        switch body {
        case .text:
            true
        case let .tool(tool):
            if case .plan? = tool.detail {
                tool.state != .asking
            } else {
                false
            }
        default:
            false
        }
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
