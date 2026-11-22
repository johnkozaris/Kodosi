import KodosiBridge
import KodosiTerminalSessions
import Testing

@Test func `a view sends its earlier size again after the terminal refused a size of the view`() {
    let delivery = ResizeDeliveryState()
    let size = { (requestId: String, rows: UInt16) in
        TerminalResizeIdentity(
            requestId: requestId, expectedRuntimeIncarnationId: "incarnation", subscriptionId: "subscription",
            subscriptionGeneration: 1, surfaceGeneration: 1, cols: 49, rows: rows, widthPixels: 490,
            heightPixels: UInt32(rows) * 20, cellWidthPixels: 10, cellHeightPixels: 20,
        )
    }
    let keyboardUp = size("up", 21)
    #expect(delivery.begin(keyboardUp, key: "49x21") == keyboardUp)
    #expect(delivery.apply(keyboardUp))
    #expect(delivery.begin(size("same", 21), key: "49x21") == nil)

    let keyboardDown = size("down", 35)
    #expect(delivery.begin(keyboardDown, key: "49x35") == keyboardDown)
    #expect(delivery.reject(keyboardDown))
    let keyboardUpAgain = size("up again", 21)
    #expect(delivery.begin(keyboardUpAgain, key: "49x21") == keyboardUpAgain)
}
