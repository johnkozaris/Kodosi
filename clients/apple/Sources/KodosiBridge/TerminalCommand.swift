public struct TerminalResizeIdentity: Equatable, Sendable {
    public let requestId: String
    public let expectedRuntimeIncarnationId: String
    public let subscriptionId: String
    public let subscriptionGeneration: UInt64
    public let surfaceGeneration: UInt64
    public let cols: UInt16
    public let rows: UInt16
    public let widthPixels: UInt32
    public let heightPixels: UInt32
    public let cellWidthPixels: UInt32
    public let cellHeightPixels: UInt32

    public init(
        requestId: String,
        expectedRuntimeIncarnationId: String,
        subscriptionId: String,
        subscriptionGeneration: UInt64,
        surfaceGeneration: UInt64,
        cols: UInt16,
        rows: UInt16,
        widthPixels: UInt32,
        heightPixels: UInt32,
        cellWidthPixels: UInt32,
        cellHeightPixels: UInt32
    ) {
        self.requestId = requestId
        self.expectedRuntimeIncarnationId = expectedRuntimeIncarnationId
        self.subscriptionId = subscriptionId
        self.subscriptionGeneration = subscriptionGeneration
        self.surfaceGeneration = surfaceGeneration
        self.cols = cols
        self.rows = rows
        self.widthPixels = widthPixels
        self.heightPixels = heightPixels
        self.cellWidthPixels = cellWidthPixels
        self.cellHeightPixels = cellHeightPixels
    }
}

enum TerminalCommand: Encodable {
    case resize(sessionId: String, identity: TerminalResizeIdentity, claim: Bool)
    case focus(
        sessionId: String,
        clientId: String,
        subscriptionGeneration: UInt64,
        requestId: String,
        expectedRuntimeIncarnationId: String
    )
    case blur(
        sessionId: String,
        clientId: String,
        subscriptionGeneration: UInt64,
        expectedRuntimeIncarnationId: String
    )

    private enum CodingKeys: String, CodingKey {
        case type, sessionId, cols, rows, widthPixels, heightPixels
        case cellWidthPixels, cellHeightPixels, surfaceGeneration
        case clientId, requestId, expectedRuntimeIncarnationId
        case subscriptionId, subscriptionGeneration, claim
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case let .resize(sessionId, identity, claim):
            try c.encode("session.resize", forKey: .type)
            try c.encode(sessionId, forKey: .sessionId)
            try c.encode(identity.cols, forKey: .cols)
            try c.encode(identity.rows, forKey: .rows)
            try c.encode(identity.widthPixels, forKey: .widthPixels)
            try c.encode(identity.heightPixels, forKey: .heightPixels)
            try c.encode(identity.cellWidthPixels, forKey: .cellWidthPixels)
            try c.encode(identity.cellHeightPixels, forKey: .cellHeightPixels)
            try c.encode(identity.surfaceGeneration, forKey: .surfaceGeneration)
            try c.encode(identity.requestId, forKey: .requestId)
            try c.encode(
                identity.expectedRuntimeIncarnationId,
                forKey: .expectedRuntimeIncarnationId
            )
            try c.encode(identity.subscriptionId, forKey: .subscriptionId)
            try c.encode(identity.subscriptionGeneration, forKey: .subscriptionGeneration)
            try c.encode(claim, forKey: .claim)
        case let .focus(sessionId, clientId, subscriptionGeneration, requestId, expectedRuntimeIncarnationId):
            try c.encode("session.focus", forKey: .type)
            try c.encode(sessionId, forKey: .sessionId)
            try c.encode(clientId, forKey: .clientId)
            try c.encode(subscriptionGeneration, forKey: .subscriptionGeneration)
            try c.encode(requestId, forKey: .requestId)
            try c.encode(expectedRuntimeIncarnationId, forKey: .expectedRuntimeIncarnationId)
        case let .blur(sessionId, clientId, subscriptionGeneration, expectedRuntimeIncarnationId):
            try c.encode("session.blur", forKey: .type)
            try c.encode(sessionId, forKey: .sessionId)
            try c.encode(clientId, forKey: .clientId)
            try c.encode(subscriptionGeneration, forKey: .subscriptionGeneration)
            try c.encode(expectedRuntimeIncarnationId, forKey: .expectedRuntimeIncarnationId)
        }
    }
}
