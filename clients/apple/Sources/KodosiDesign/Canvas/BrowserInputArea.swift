import KodosiBridge
import SwiftUI

struct BrowserInputArea {
    let feed: BrowserFeed
    let typing: Bool
    let ask: CanvasAsk?
}

extension View {
    func browserPageMenu(feed: BrowserFeed, ask: CanvasAsk?) -> some View {
        #if os(macOS)
            self
        #else
            modifier(BrowserPageMenu(feed: feed, ask: ask))
        #endif
    }
}

private extension BrowserFeed {
    func type(_ text: String) {
        for character in text {
            if let key = BrowserKeys.key(character: character) {
                send(.key(.keyDown, key: key.named ?? String(character), code: key.code, text: String(character), keyCode: key.windows))
                send(.key(.keyUp, key: key.named ?? String(character), code: key.code, keyCode: key.windows))
            } else {
                send(.text(String(character)))
            }
        }
    }

    func press(_ key: BrowserKeys.Key, typed: String?, down: Bool, modifiers: BrowserInput.Modifiers) {
        let name = key.named ?? typed ?? ""
        let text = down ? (key.code == "Space" ? " " : key.named == "Enter" ? "\r" : key.named == nil ? typed : nil) : nil
        send(.key(down ? .keyDown : .keyUp, key: name, code: key.code, text: text, keyCode: key.windows, modifiers: modifiers))
    }
}

extension BrowserKeys {
    /// An on-screen keyboard gives only text, so the key is the one that types this character on
    /// a US keyboard.
    static func key(character: Character) -> Key? {
        guard let ascii = character.asciiValue else { return nil }
        switch character {
        case "a" ... "z", "A" ... "Z":
            let upper = Character(character.uppercased())
            return Key(code: "Key\(upper)", named: nil, windows: Int(upper.asciiValue ?? ascii))
        case "0" ... "9":
            return Key(code: "Digit\(character)", named: nil, windows: Int(ascii))
        case " ":
            return Key(code: "Space", named: " ", windows: 32)
        default:
            return nil
        }
    }
}

#if os(macOS)
    import AppKit

    extension BrowserInputArea: NSViewRepresentable {
        func makeNSView(context _: Context) -> BrowserInputView {
            BrowserInputView(feed: feed)
        }

        func updateNSView(_ view: BrowserInputView, context _: Context) {
            view.feed = feed
            view.ask = ask
            view.show(shape: feed.window.shape)
            view.show(menu: feed.window.menu)
        }
    }

    final class BrowserInputView: NSView {
        var feed: BrowserFeed
        var ask: CanvasAsk?
        private var shape = ""
        private var pageMenu: BrowserWindow.Menu?
        private var shownMenu: NSMenu?

        func show(shape: String) {
            if shape != self.shape {
                self.shape = shape
                window?.invalidateCursorRects(for: self)
            }
        }

        func show(menu: BrowserWindow.Menu?) {
            guard menu != pageMenu else { return }
            pageMenu = menu
            if let shownMenu {
                shownMenu.cancelTracking()
            } else if let menu {
                Task { [weak self] in self?.pop(menu) }
            }
        }

        /// A page opens its list on the press, so the menu can come while the button is still
        /// down; the menu then takes the release, and the page gets it here instead.
        private func pop(_ menu: BrowserWindow.Menu) {
            guard menu == pageMenu, shownMenu == nil, window != nil,
                  let place = feed.viewPoint(x: menu.x, y: menu.y, in: bounds.size) else { return }
            let held = NSEvent.pressedMouseButtons
            for (bit, button) in [(0, BrowserInput.MouseButton.left), (1, .right), (2, .middle)] where held & 1 << bit != 0 {
                feed.send(.mouse(.mouseReleased, x: menu.x, y: menu.y, button: button, clicks: 1))
            }
            let native = menu.context.map { context in contextMenu(menu, context) } ?? list(menu)
            shownMenu = native
            let chose = native.popUp(positioning: native.items.first { $0.state == .on }, at: place, in: self)
            shownMenu = nil
            if menu == pageMenu {
                if !chose {
                    feed.send(.choose(menu: menu.id, choice: nil))
                }
            } else if let next = pageMenu {
                Task { [weak self] in self?.pop(next) }
            }
        }

        private func list(_ menu: BrowserWindow.Menu) -> NSMenu {
            let native = NSMenu()
            native.autoenablesItems = false
            for (index, item) in menu.items.enumerated() {
                let entry = MenuAction.item(item.label, enabled: item.enabled) { [feed] in
                    feed.send(.choose(menu: menu.id, choice: index))
                }
                entry.state = item.chosen ? .on : .off
                native.addItem(entry)
            }
            return native
        }

        private func contextMenu(_ menu: BrowserWindow.Menu, _ context: BrowserWindow.MenuContext) -> NSMenu {
            let native = NSMenu()
            native.autoenablesItems = false
            let feed = feed
            native.addItem(MenuAction.item(String(localized: "Copy", bundle: .module), enabled: !context.selection.isEmpty) {
                feed.send(.choose(menu: menu.id, choice: BrowserWindow.MenuContext.copySelection))
            })
            if context.link != nil {
                native.addItem(MenuAction.item(String(localized: "Copy Link", bundle: .module)) {
                    feed.send(.choose(menu: menu.id, choice: BrowserWindow.MenuContext.copyLink))
                })
            }
            let paste = NSMenuItem(title: String(localized: "Paste", bundle: .module), action: #selector(paste(_:)), keyEquivalent: "")
            paste.target = self
            paste.representedObject = menu.id
            paste.isEnabled = NSPasteboard.general.availableType(from: [.string]) != nil
            native.addItem(paste)
            if let ask {
                native.addItem(.separator())
                native.addItem(MenuAction.item(ask.title) {
                    feed.send(.choose(menu: menu.id, choice: nil))
                    ask.send(context.words)
                })
            }
            return native
        }

        init(feed: BrowserFeed) {
            self.feed = feed
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            nil
        }

        override var isFlipped: Bool {
            true
        }

        override var acceptsFirstResponder: Bool {
            true
        }

        override func acceptsFirstMouse(for _: NSEvent?) -> Bool {
            true
        }

        private static let cursors: [String: NSCursor] = [
            "pointer": .pointingHand,
            "text": .iBeam,
            "vertical-text": .iBeamCursorForVerticalLayout,
            "crosshair": .crosshair,
            "cell": .crosshair,
            "context-menu": .contextualMenu,
            "alias": .dragLink,
            "copy": .dragCopy,
            "grab": .openHand,
            "grabbing": .closedHand,
            "not-allowed": .operationNotAllowed,
            "col-resize": .columnResize,
            "ew-resize": .columnResize,
            "w-resize": .columnResize(directions: .left),
            "e-resize": .columnResize(directions: .right),
            "row-resize": .rowResize,
            "ns-resize": .rowResize,
            "n-resize": .rowResize(directions: .up),
            "s-resize": .rowResize(directions: .down),
            "ne-resize": .frameResize(position: .topRight, directions: .all),
            "nesw-resize": .frameResize(position: .topRight, directions: .all),
            "nw-resize": .frameResize(position: .topLeft, directions: .all),
            "nwse-resize": .frameResize(position: .topLeft, directions: .all),
            "se-resize": .frameResize(position: .bottomRight, directions: .all),
            "sw-resize": .frameResize(position: .bottomLeft, directions: .all),
            "zoom-in": .zoomIn,
            "zoom-out": .zoomOut,
        ]

        override func resetCursorRects() {
            addCursorRect(bounds, cursor: Self.cursors[feed.window.shape] ?? .arrow)
        }

        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            for area in trackingAreas {
                removeTrackingArea(area)
            }
            addTrackingArea(NSTrackingArea(rect: .zero, options: [.mouseMoved, .activeInKeyWindow, .inVisibleRect], owner: self))
        }

        override func mouseDown(with event: NSEvent) {
            window?.makeFirstResponder(self)
            mouse(.mousePressed, event, button: .left)
        }

        override func mouseUp(with event: NSEvent) {
            mouse(.mouseReleased, event, button: .left)
        }

        override func rightMouseDown(with event: NSEvent) {
            window?.makeFirstResponder(self)
            mouse(.mousePressed, event, button: .right)
        }

        override func rightMouseUp(with event: NSEvent) {
            mouse(.mouseReleased, event, button: .right)
        }

        override func otherMouseDown(with event: NSEvent) {
            mouse(.mousePressed, event, button: .middle)
        }

        override func otherMouseUp(with event: NSEvent) {
            mouse(.mouseReleased, event, button: .middle)
        }

        override func mouseDragged(with event: NSEvent) {
            mouse(.mouseMoved, event, button: .left)
        }

        override func rightMouseDragged(with event: NSEvent) {
            mouse(.mouseMoved, event, button: .right)
        }

        override func otherMouseDragged(with event: NSEvent) {
            mouse(.mouseMoved, event, button: .middle)
        }

        override func mouseMoved(with event: NSEvent) {
            mouse(.mouseMoved, event, button: .none)
        }

        override func scrollWheel(with event: NSEvent) {
            guard let point = feed.browserPoint(convert(event.locationInWindow, from: nil), in: bounds.size) else { return }
            let scale: CGFloat = event.hasPreciseScrollingDeltas ? 1 : 40
            feed.send(.mouse(.mouseWheel, x: point.x, y: point.y, deltaX: Double(-event.scrollingDeltaX * scale),
                             deltaY: Double(-event.scrollingDeltaY * scale), modifiers: Self.modifiers(event.modifierFlags)))
        }

        override func keyDown(with event: NSEvent) {
            key(event, down: true)
        }

        override func keyUp(with event: NSEvent) {
            key(event, down: false)
        }

        /// macOS asks the person before an app reads the pasteboard, unless one of their pastes
        /// reads it, so a menu looks only at the type and its Paste is the paste action.
        @objc func paste(_ sender: Any?) {
            if let menu = (sender as? NSMenuItem)?.representedObject as? Int {
                feed.send(.choose(menu: menu, choice: nil))
            }
            if let text = NSPasteboard.general.string(forType: .string), !text.isEmpty {
                feed.send(.text(text))
            }
        }

        @objc func copy(_: Any?) {
            command(keyCode: 8, "c")
        }

        @objc func cut(_: Any?) {
            command(keyCode: 7, "x")
        }

        override func selectAll(_: Any?) {
            command(keyCode: 0, "a")
        }

        private func command(keyCode: UInt16, _ letter: String) {
            guard let key = BrowserKeys.key(macKeyCode: keyCode) else { return }
            feed.press(key, typed: letter, down: true, modifiers: .meta)
            feed.press(key, typed: letter, down: false, modifiers: .meta)
        }

        private func mouse(_ type: BrowserInput.MouseEvent, _ event: NSEvent, button: BrowserInput.MouseButton) {
            guard let point = feed.browserPoint(convert(event.locationInWindow, from: nil), in: bounds.size) else { return }
            let clicks = type == .mouseMoved ? 0 : event.clickCount
            feed.send(.mouse(type, x: point.x, y: point.y, button: button, clicks: clicks, modifiers: Self.modifiers(event.modifierFlags)))
        }

        private func key(_ event: NSEvent, down: Bool) {
            guard let key = BrowserKeys.key(macKeyCode: event.keyCode) else { return }
            let held = Self.modifiers(event.modifierFlags)
            let typed = held.isDisjoint(with: [.control, .meta]) ? event.characters : event.charactersIgnoringModifiers
            feed.press(key, typed: typed?.isEmpty == false ? typed : nil, down: down, modifiers: held)
        }

        static func modifiers(_ flags: NSEvent.ModifierFlags) -> BrowserInput.Modifiers {
            var held: BrowserInput.Modifiers = []
            if flags.contains(.option) {
                held.insert(.alt)
            }
            if flags.contains(.control) {
                held.insert(.control)
            }
            if flags.contains(.command) {
                held.insert(.meta)
            }
            if flags.contains(.shift) {
                held.insert(.shift)
            }
            return held
        }
    }

    @MainActor
    private final class MenuAction: NSObject {
        private let run: () -> Void

        private init(_ run: @escaping () -> Void) {
            self.run = run
        }

        @objc private func choose() {
            run()
        }

        /// A menu item holds its target weakly, so the item also keeps it.
        static func item(_ title: String, enabled: Bool = true, run: @escaping () -> Void) -> NSMenuItem {
            let action = MenuAction(run)
            let item = NSMenuItem(title: title, action: #selector(choose), keyEquivalent: "")
            item.target = action
            item.representedObject = action
            item.isEnabled = enabled
            return item
        }
    }

#else
    import UIKit

    extension BrowserInputArea: UIViewRepresentable {
        func makeUIView(context _: Context) -> BrowserInputView {
            BrowserInputView(feed: feed)
        }

        func updateUIView(_ view: BrowserInputView, context _: Context) {
            view.feed = feed
            view.wantsKeyboard(typing)
        }
    }

    struct BrowserPageMenu: ViewModifier {
        let feed: BrowserFeed
        let ask: CanvasAsk?
        @State private var answered: Int?

        func body(content: Content) -> some View {
            let menu = feed.window.menu.flatMap { $0.id == answered ? nil : $0 }
            content.confirmationDialog(
                Text(verbatim: ""),
                isPresented: Binding(get: { menu != nil }, set: { shown in
                    if !shown, let menu {
                        answer(menu, nil)
                    }
                }),
                titleVisibility: .hidden,
                presenting: menu,
            ) { menu in
                if let context = menu.context {
                    Button(String(localized: "Copy", bundle: .module)) {
                        answer(menu, BrowserWindow.MenuContext.copySelection)
                    }
                    .disabled(context.selection.isEmpty)
                    if context.link != nil {
                        Button(String(localized: "Copy Link", bundle: .module)) {
                            answer(menu, BrowserWindow.MenuContext.copyLink)
                        }
                    }
                    Button(String(localized: "Paste", bundle: .module)) {
                        answer(menu, nil)
                        if let text = UIPasteboard.general.string, !text.isEmpty {
                            feed.send(.text(text))
                        }
                    }
                    if let ask {
                        Button(ask.title) {
                            answer(menu, nil)
                            ask.send(context.words)
                        }
                    }
                } else {
                    ForEach(Array(menu.items.enumerated()), id: \.offset) { index, item in
                        Button {
                            answer(menu, index)
                        } label: {
                            Text(verbatim: item.chosen ? "✓ \(item.label)" : item.label)
                        }
                        .disabled(!item.enabled)
                    }
                }
                Button(String(localized: "Cancel", bundle: .module), role: .cancel) {
                    answer(menu, nil)
                }
            }
        }

        private func answer(_ menu: BrowserWindow.Menu, _ choice: Int?) {
            guard answered != menu.id else { return }
            answered = menu.id
            feed.send(.choose(menu: menu.id, choice: choice))
        }
    }

    final class BrowserInputView: UIView, UIKeyInput {
        private static let dragDistance: CGFloat = 10
        private static let slowestMomentum: CGFloat = 20

        var feed: BrowserFeed
        private var keyboard = false
        private var taps = BrowserTaps()
        private var holdStart: CGPoint?
        private var dragging = false
        private var pointerButton: BrowserInput.MouseButton?
        private var momentum: CADisplayLink?
        private var momentumVelocity = CGPoint.zero
        private var momentumPoint = CGPoint.zero
        private var touchStoppedMomentum = false

        init(feed: BrowserFeed) {
            self.feed = feed
            super.init(frame: .zero)
            backgroundColor = .clear
            let hold = UILongPressGestureRecognizer(target: self, action: #selector(held))
            let tap = UITapGestureRecognizer(target: self, action: #selector(tapped))
            tap.require(toFail: hold)
            let pair = UITapGestureRecognizer(target: self, action: #selector(pairTapped))
            pair.numberOfTouchesRequired = 2
            let scroll = UIPanGestureRecognizer(target: self, action: #selector(scrolled))
            for touch in [hold, tap, pair, scroll] {
                touch.allowedTouchTypes = [NSNumber(value: UITouch.TouchType.direct.rawValue)]
                addGestureRecognizer(touch)
            }
            let pointerScroll = UIPanGestureRecognizer(target: self, action: #selector(pointerScrolled))
            pointerScroll.allowedTouchTypes = []
            pointerScroll.allowedScrollTypesMask = .all
            addGestureRecognizer(pointerScroll)
            addGestureRecognizer(UIHoverGestureRecognizer(target: self, action: #selector(hovered)))
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            nil
        }

        /// SwiftUI updates its views while UIKit changes the first responder, so a view that takes
        /// the keyboard in an update starts the next update, and takes the keyboard from the
        /// address field. The view takes or gives back the keyboard only when the person switches
        /// it, after the update.
        func wantsKeyboard(_ wanted: Bool) {
            guard wanted != keyboard else { return }
            keyboard = wanted
            followKeyboard()
        }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            if window == nil {
                stopMomentum()
            }
            followKeyboard()
        }

        private func followKeyboard() {
            Task { [weak self] in
                await Task.yield()
                guard let self, window != nil else { return }
                if keyboard, !isFirstResponder {
                    becomeFirstResponder()
                } else if !keyboard, isFirstResponder {
                    resignFirstResponder()
                }
            }
        }

        override var canBecomeFirstResponder: Bool {
            true
        }

        var hasText: Bool {
            true
        }

        var autocorrectionType = UITextAutocorrectionType.no
        var autocapitalizationType = UITextAutocapitalizationType.none

        func insertText(_ text: String) {
            if text == "\n", let enter = BrowserKeys.key(hidUsage: 0x28) {
                feed.press(enter, typed: nil, down: true, modifiers: [])
                feed.press(enter, typed: nil, down: false, modifiers: [])
            } else {
                feed.type(text)
            }
        }

        func deleteBackward() {
            guard let backspace = BrowserKeys.key(hidUsage: 0x2A) else { return }
            feed.press(backspace, typed: nil, down: true, modifiers: [])
            feed.press(backspace, typed: nil, down: false, modifiers: [])
        }

        override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            if !handle(presses, down: true) {
                super.pressesBegan(presses, with: event)
            }
        }

        override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
            if !handle(presses, down: false) {
                super.pressesEnded(presses, with: event)
            }
        }

        private func handle(_ presses: Set<UIPress>, down: Bool) -> Bool {
            var handled = false
            for press in presses {
                guard let pressed = press.key, let key = BrowserKeys.key(hidUsage: pressed.keyCode.rawValue) else { continue }
                let held = Self.modifiers(pressed.modifierFlags)
                let typed = held.isDisjoint(with: [.control, .meta]) ? pressed.characters : pressed.charactersIgnoringModifiers
                feed.press(key, typed: typed.isEmpty ? nil : typed, down: down, modifiers: held)
                handled = true
            }
            return handled
        }

        override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
            touchStoppedMomentum = momentum != nil
            stopMomentum()
            if !pointer(touches, .mousePressed, event) {
                super.touchesBegan(touches, with: event)
            }
        }

        override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
            if !pointer(touches, .mouseMoved, event) {
                super.touchesMoved(touches, with: event)
            }
        }

        override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
            if !pointer(touches, .mouseReleased, event) {
                super.touchesEnded(touches, with: event)
            }
        }

        override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
            if !pointer(touches, .mouseReleased, event) {
                super.touchesCancelled(touches, with: event)
            }
        }

        /// A mouse or a trackpad presses, drags and clicks as on a computer.
        private func pointer(_ touches: Set<UITouch>, _ type: BrowserInput.MouseEvent, _ event: UIEvent?) -> Bool {
            guard let touch = touches.first(where: { $0.type == .indirectPointer }) else { return false }
            if type == .mousePressed {
                pointerButton = event?.buttonMask.contains(.secondary) == true ? .right : .left
            }
            guard let button = pointerButton else { return true }
            if type == .mouseReleased {
                pointerButton = nil
            }
            mouse(type, at: touch.location(in: self), button: button, clicks: type == .mouseMoved ? 0 : touch.tapCount,
                  modifiers: Self.modifiers(event?.modifierFlags ?? []))
            return true
        }

        @objc private func tapped(_ gesture: UITapGestureRecognizer) {
            guard !touchStoppedMomentum else { return }
            let location = gesture.location(in: self)
            click(at: location, button: .left, clicks: taps.click(at: location, time: CACurrentMediaTime()))
        }

        @objc private func pairTapped(_ gesture: UITapGestureRecognizer) {
            taps.forget()
            click(at: gesture.location(in: self), button: .right)
        }

        /// A press that rests and then moves drags with the left button, which selects text as a
        /// mouse does; a press that rests and lets go opens the menu of the page.
        @objc private func held(_ gesture: UILongPressGestureRecognizer) {
            let location = gesture.location(in: self)
            switch gesture.state {
            case .began:
                taps.forget()
                holdStart = location
                UISelectionFeedbackGenerator().selectionChanged()
            case .changed:
                guard let start = holdStart else { return }
                if !dragging {
                    guard hypot(location.x - start.x, location.y - start.y) > Self.dragDistance else { return }
                    guard mouse(.mousePressed, at: start, button: .left, clicks: 1) else {
                        holdStart = nil
                        return
                    }
                    dragging = true
                }
                mouse(.mouseMoved, at: location, button: .left)
            case .ended, .cancelled:
                if dragging {
                    mouse(.mouseReleased, at: location, button: .left, clicks: 1)
                } else if gesture.state == .ended, let start = holdStart {
                    click(at: start, button: .right)
                }
                holdStart = nil
                dragging = false
            default:
                break
            }
        }

        @objc private func scrolled(_ gesture: UIPanGestureRecognizer) {
            guard [.began, .changed, .ended].contains(gesture.state) else { return }
            let location = gesture.location(in: self)
            scroll(by: gesture.translation(in: self), at: location)
            gesture.setTranslation(.zero, in: self)
            if gesture.state == .ended {
                startMomentum(velocity: gesture.velocity(in: self), at: location)
            }
        }

        @objc private func pointerScrolled(_ gesture: UIPanGestureRecognizer) {
            guard gesture.state == .began || gesture.state == .changed else { return }
            stopMomentum()
            scroll(by: gesture.translation(in: self), at: gesture.location(in: self))
            gesture.setTranslation(.zero, in: self)
        }

        @objc private func hovered(_ gesture: UIHoverGestureRecognizer) {
            guard gesture.state == .began || gesture.state == .changed else { return }
            mouse(.mouseMoved, at: gesture.location(in: self), button: .none, modifiers: Self.modifiers(gesture.modifierFlags))
        }

        /// The page moves as far as the finger, so the place under the finger stays under it.
        private func scroll(by moved: CGPoint, at location: CGPoint) {
            let page = feed.window.size
            guard moved != .zero, page.width > 0, page.height > 0, bounds.width > 0, bounds.height > 0,
                  let point = feed.browserPoint(location, in: bounds.size) else { return }
            let fit = min(bounds.width / page.width, bounds.height / page.height)
            feed.send(.mouse(.mouseWheel, x: point.x, y: point.y, deltaX: Double(-moved.x / fit), deltaY: Double(-moved.y / fit)))
        }

        private func startMomentum(velocity: CGPoint, at location: CGPoint) {
            guard hypot(velocity.x, velocity.y) > Self.slowestMomentum else { return }
            momentumVelocity = velocity
            momentumPoint = location
            let link = CADisplayLink(target: self, selector: #selector(momentumFrame))
            link.preferredFrameRateRange = CAFrameRateRange(minimum: 30, maximum: 60, preferred: 60)
            link.add(to: .main, forMode: .common)
            momentum = link
        }

        @objc private func momentumFrame(_ link: CADisplayLink) {
            let elapsed = CGFloat(max(link.targetTimestamp - link.timestamp, 0))
            let decay = pow(UIScrollView.DecelerationRate.normal.rawValue, elapsed * 1000)
            momentumVelocity = CGPoint(x: momentumVelocity.x * decay, y: momentumVelocity.y * decay)
            guard hypot(momentumVelocity.x, momentumVelocity.y) > Self.slowestMomentum else {
                stopMomentum()
                return
            }
            scroll(by: CGPoint(x: momentumVelocity.x * elapsed, y: momentumVelocity.y * elapsed), at: momentumPoint)
        }

        private func stopMomentum() {
            momentum?.invalidate()
            momentum = nil
        }

        private func click(at location: CGPoint, button: BrowserInput.MouseButton, clicks: Int = 1) {
            guard mouse(.mousePressed, at: location, button: button, clicks: clicks) else { return }
            mouse(.mouseReleased, at: location, button: button, clicks: clicks)
        }

        @discardableResult
        private func mouse(_ type: BrowserInput.MouseEvent, at location: CGPoint, button: BrowserInput.MouseButton,
                           clicks: Int = 0, modifiers: BrowserInput.Modifiers = []) -> Bool
        {
            guard let point = feed.browserPoint(location, in: bounds.size) else { return false }
            feed.send(.mouse(type, x: point.x, y: point.y, button: button, clicks: clicks, modifiers: modifiers))
            return true
        }

        static func modifiers(_ flags: UIKeyModifierFlags) -> BrowserInput.Modifiers {
            var held: BrowserInput.Modifiers = []
            if flags.contains(.alternate) {
                held.insert(.alt)
            }
            if flags.contains(.control) {
                held.insert(.control)
            }
            if flags.contains(.command) {
                held.insert(.meta)
            }
            if flags.contains(.shift) {
                held.insert(.shift)
            }
            return held
        }
    }
#endif
