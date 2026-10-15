import Foundation
import KodosiBridge
import Testing

enum RuntimeContract {
    private struct Shape {
        let required: Set<String>
        let allowed: Set<String>
    }

    private static let schema: [String: Any] = {
        let url = URL(filePath: #filePath)
            .deletingLastPathComponent()
            .appending(path: "../../../protocol/desktop-runtime.json")
            .standardizedFileURL
        guard let data = try? Data(contentsOf: url),
              let root = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return [:] }
        return root
    }()

    private static let shapes: [String: Shape] = load()

    static var version: Int? {
        schema["protocolVersion"] as? Int
    }

    static func verify(_ data: Data, sourceLocation: SourceLocation = #_sourceLocation) {
        if let violation = violation(in: data) {
            Issue.record(Comment(rawValue: violation), sourceLocation: sourceLocation)
        }
    }

    static func violation(in data: Data) -> String? {
        guard let command = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let type = command["type"] as? String
        else { return "The app sent something that is not a runtime command." }
        guard let shape = shapes[type] else { return "The runtime has no command \(type)." }
        let keys = Set(command.keys)
        let missing = shape.required.subtracting(keys).sorted()
        let unknown = keys.subtracting(shape.allowed).sorted()
        guard !missing.isEmpty || !unknown.isEmpty else { return nil }
        return "\(type) is missing \(missing) and has unknown fields \(unknown)."
    }

    private static func load() -> [String: Shape] {
        guard let commands = schema["commands"] as? [String: Any],
              let variants = commands["oneOf"] as? [[String: Any]]
        else { return [:] }
        var shapes: [String: Shape] = [:]
        for variant in variants {
            guard let properties = variant["properties"] as? [String: Any],
                  let type = (properties["type"] as? [String: Any])?["const"] as? String
            else { continue }
            shapes[type] = Shape(
                required: Set(variant["required"] as? [String] ?? []),
                allowed: Set(properties.keys)
            )
        }
        return shapes
    }
}

struct RuntimeContractTests {
    @Test func theSchemaListsTheCommandsTheAppSends() {
        let open = #"{"type":"session.openRemote","sessionId":"s","accountUserId":null,"accountEpoch":0}"#
        #expect(RuntimeContract.violation(in: Data(open.utf8)) != nil)
        let complete = #"{"type":"session.openRemote","sessionId":"s","requestId":"r","accountUserId":null,"accountEpoch":0}"#
        #expect(RuntimeContract.violation(in: Data(complete.utf8)) == nil)
    }

    @Test func theAppExpectsTheSchemaVersion() {
        #expect(RuntimeContract.version == Int(RuntimeHandle.expectedProtocolVersion))
    }
}
