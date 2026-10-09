import Foundation
import KodosiKit

enum TerminalInputFailure: Equatable, Sendable {
    case budgetExceeded(perSession: Bool)
    case runtimeUnavailable
    case rejected(code: Int32)

    var userMessage: String {
        switch self {
        case let .budgetExceeded(perSession):
            if perSession {
                return String(localized: "This terminal still waits for earlier typing. Your paste was not sent.")
            }
            return String(localized: "That paste is too large. It was not sent.")
        case .runtimeUnavailable:
            return String(localized: "Kodosi stopped before it sent your typing.")
        case let .rejected(code):
            return String(localized: "Your typing was not sent (error \(code)).")
        }
    }
}

final class TerminalInputQueue: @unchecked Sendable {
    static let shared = TerminalInputQueue(
        perSessionBudget: defaultPerSessionBudget,
        globalBudget: defaultGlobalBudget,
        coalesceLimit: 64 * 1024,
        busyDelays: defaultBusyDelays
    )

    typealias Sender = @Sendable (Data) -> Int32?
    typealias FailureHandler = @Sendable (TerminalInputFailure) -> Void

    private struct Chunk {
        var data: Data
        let send: Sender
        let onFailure: FailureHandler
        let coalescingToken: UInt64
        var busyAttempt = 0
    }

    private struct Ring<Element> {
        private var storage: [Element?] = Array(repeating: nil, count: 8)
        private var head = 0
        private(set) var count = 0

        var isEmpty: Bool {
            count == 0
        }

        mutating func append(_ element: Element) {
            ensureCapacity()
            storage[(head + count) % storage.count] = element
            count += 1
        }

        mutating func prepend(_ element: Element) {
            ensureCapacity()
            head = (head - 1 + storage.count) % storage.count
            storage[head] = element
            count += 1
        }

        mutating func popFirst() -> Element? {
            guard count > 0 else { return nil }
            let element = storage[head]
            storage[head] = nil
            head = (head + 1) % storage.count
            count -= 1
            return element
        }

        mutating func mutateLast(_ body: (inout Element) -> Void) -> Bool {
            guard count > 0 else { return false }
            let index = (head + count - 1) % storage.count
            guard var element = storage[index] else { return false }
            body(&element)
            storage[index] = element
            return true
        }

        private mutating func ensureCapacity() {
            guard count == storage.count else { return }
            var expanded: [Element?] = Array(repeating: nil, count: storage.count * 2)
            for offset in 0 ..< count {
                expanded[offset] = storage[(head + offset) % storage.count]
            }
            storage = expanded
            head = 0
        }
    }

    private struct SessionQueue {
        let epoch: UInt64
        var chunks = Ring<Chunk>()
        var byteCount = 0
        var inFlight = false
        var retryBlocked = false
    }

    private struct Candidate {
        let sessionId: String
        let epoch: UInt64
        var chunk: Chunk
    }

    private static let defaultPerSessionBudget = 8 * 1024 * 1024
    private static let defaultGlobalBudget = 32 * 1024 * 1024
    private static let defaultBusyDelays: [Duration] = [
        .milliseconds(5),
        .milliseconds(10),
        .milliseconds(20),
        .milliseconds(40),
        .milliseconds(80),
        .milliseconds(160),
        .milliseconds(250),
    ]

    private let lock = NSLock()
    private let perSessionBudget: Int
    private let globalBudget: Int
    private let coalesceLimit: Int
    private let busyDelays: [Duration]
    private var sessions: [String: SessionQueue] = [:]
    private var ready = Ring<String>()
    private var readyIds: Set<String> = []
    private var totalBytes = 0
    private var draining = false
    private var nextEpoch: UInt64 = 0

    init(
        perSessionBudget: Int,
        globalBudget: Int,
        coalesceLimit: Int,
        busyDelays: [Duration]
    ) {
        precondition(perSessionBudget > 0)
        precondition(globalBudget >= perSessionBudget)
        precondition(coalesceLimit > 0)
        precondition(!busyDelays.isEmpty)
        self.perSessionBudget = perSessionBudget
        self.globalBudget = globalBudget
        self.coalesceLimit = coalesceLimit
        self.busyDelays = busyDelays
    }

    func enqueue(
        _ data: Data,
        sessionId: String,
        coalescingToken: UInt64,
        send: @escaping Sender,
        onFailure: @escaping FailureHandler
    ) {
        guard !data.isEmpty else { return }
        var shouldStart = false
        var rejection: TerminalInputFailure?
        lock.withLock {
            var session: SessionQueue
            if let existing = sessions[sessionId] {
                session = existing
            } else {
                nextEpoch &+= 1
                session = SessionQueue(epoch: nextEpoch)
            }
            if session.byteCount + data.count > perSessionBudget {
                rejection = .budgetExceeded(perSession: true)
                return
            }
            if totalBytes + data.count > globalBudget {
                rejection = .budgetExceeded(perSession: false)
                return
            }

            var didCoalesce = false
            _ = session.chunks.mutateLast { tail in
                guard tail.coalescingToken == coalescingToken,
                      tail.data.count + data.count <= coalesceLimit
                else { return }
                tail.data.append(data)
                didCoalesce = true
            }
            if !didCoalesce {
                session.chunks.append(Chunk(
                    data: data,
                    send: send,
                    onFailure: onFailure,
                    coalescingToken: coalescingToken
                ))
            }
            session.byteCount += data.count
            totalBytes += data.count
            if !session.inFlight, !session.retryBlocked, readyIds.insert(sessionId).inserted {
                ready.append(sessionId)
            }
            sessions[sessionId] = session
            if !draining {
                draining = true
                shouldStart = true
            }
        }

        if let rejection {
            onFailure(rejection)
            return
        }
        if shouldStart {
            Task { await drain() }
        }
    }

    func cancel(sessionId: String) {
        lock.withLock {
            guard let session = sessions.removeValue(forKey: sessionId) else { return }
            totalBytes -= session.byteCount
            readyIds.remove(sessionId)
        }
    }

    private func drain() async {
        while let candidate = takeCandidate() {
            let result = candidate.chunk.send(candidate.chunk.data)
            switch result {
            case Int32(KODOSI_FFI_OK)?:
                finish(candidate, failure: nil)
            case Int32(KODOSI_FFI_BUSY)?:
                retry(candidate)
            case nil:
                finish(candidate, failure: .runtimeUnavailable)
            case let code?:
                finish(candidate, failure: .rejected(code: code))
            }
            await Task.yield()
        }
    }

    private func takeCandidate() -> Candidate? {
        lock.withLock {
            while let sessionId = ready.popFirst() {
                readyIds.remove(sessionId)
                guard var session = sessions[sessionId],
                      !session.inFlight,
                      !session.retryBlocked,
                      let chunk = session.chunks.popFirst()
                else {
                    continue
                }
                session.inFlight = true
                sessions[sessionId] = session
                return Candidate(sessionId: sessionId, epoch: session.epoch, chunk: chunk)
            }
            draining = false
            return nil
        }
    }

    private func finish(_ candidate: Candidate, failure: TerminalInputFailure?) {
        var shouldStart = false
        lock.withLock {
            guard var session = sessions[candidate.sessionId],
                  session.epoch == candidate.epoch
            else { return }
            session.inFlight = false
            session.byteCount -= candidate.chunk.data.count
            totalBytes -= candidate.chunk.data.count
            if session.byteCount == 0, session.chunks.isEmpty {
                sessions.removeValue(forKey: candidate.sessionId)
            } else {
                scheduleReady(candidate.sessionId, session: &session)
                sessions[candidate.sessionId] = session
            }
            if !draining, !readyIds.isEmpty {
                draining = true
                shouldStart = true
            }
        }
        if let failure {
            candidate.chunk.onFailure(failure)
        }
        if shouldStart {
            Task { await drain() }
        }
    }

    private func retry(_ candidate: Candidate) {
        var retried = candidate.chunk
        let delay = busyDelays[min(retried.busyAttempt, busyDelays.count - 1)]
        retried.busyAttempt += 1
        lock.withLock {
            guard var session = sessions[candidate.sessionId],
                  session.epoch == candidate.epoch
            else { return }
            session.inFlight = false
            session.retryBlocked = true
            session.chunks.prepend(retried)
            sessions[candidate.sessionId] = session
        }
        Task { [weak self] in
            try? await Task.sleep(for: delay)
            guard !Task.isCancelled else { return }
            self?.unblockRetry(sessionId: candidate.sessionId, epoch: candidate.epoch)
        }
    }

    private func unblockRetry(sessionId: String, epoch: UInt64) {
        var shouldStart = false
        lock.withLock {
            guard var session = sessions[sessionId], session.epoch == epoch else { return }
            session.retryBlocked = false
            scheduleReady(sessionId, session: &session)
            sessions[sessionId] = session
            if !draining, readyIds.contains(sessionId) {
                draining = true
                shouldStart = true
            }
        }
        if shouldStart {
            Task { await drain() }
        }
    }

    private func scheduleReady(_ sessionId: String, session: inout SessionQueue) {
        guard !session.inFlight,
              !session.retryBlocked,
              !session.chunks.isEmpty,
              readyIds.insert(sessionId).inserted
        else {
            return
        }
        ready.append(sessionId)
    }
}
