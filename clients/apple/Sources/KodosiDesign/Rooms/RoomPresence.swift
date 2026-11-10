import KodosiBridge
import SwiftUI

public extension View {
    /// Opens a room while this view shows it: loads its people and latest messages, and keeps its
    /// people fresh.
    func showsRoom(_ roomId: String, in rooms: RoomStore) -> some View {
        modifier(RoomShown(roomId: roomId, rooms: rooms))
    }

    /// Tells the room that this view shows one of its parts.
    func showsRoomPart(_ part: RoomPart, of state: RoomViewState) -> some View {
        onAppear { state.show(part) }
            .onDisappear { state.hide(part) }
    }
}

private struct RoomShown: ViewModifier {
    let roomId: String
    let rooms: RoomStore

    func body(content: Content) -> some View {
        content
            .task(id: roomId) { await rooms.open(roomId) }
            .onAppear { rooms.visibleRoomId = roomId }
            .onChange(of: roomId) { _, roomId in rooms.visibleRoomId = roomId }
            .onDisappear {
                if rooms.visibleRoomId == roomId {
                    rooms.visibleRoomId = nil
                }
            }
    }
}
