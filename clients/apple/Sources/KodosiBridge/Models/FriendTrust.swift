import Foundation

/// A person that you are about to trust as a friend: by their invite or username, or by their
/// request.
public enum FriendTrust: Equatable, Sendable {
    case add(String)
    case accept(FriendRequestEntry)

    public var handle: String? {
        switch self {
        case let .accept(request): request.handle
        case let .add(text): PeopleStore.handle(in: text)
        }
    }

    public var name: String? {
        switch self {
        case let .accept(request): request.nameWithHandle
        case .add: handle.map { "@\($0)" }
        }
    }
}
