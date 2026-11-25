#if os(macOS)
    import AppKit
    import QuartzCore
    import SwiftUI

    /// Displays the local macOS browser's Core Animation context directly via the private
    /// `CALayerHost` API (matching how Chromium composites GPU-process layers), fitted like the
    /// video view.
    struct BrowserLayerView: NSViewRepresentable {
        let context: UInt32
        let page: CGSize

        func makeNSView(context _: Context) -> LayerSurface {
            LayerSurface()
        }

        func updateNSView(_ view: LayerSurface, context _: Context) {
            view.show(context, page: page)
        }
    }

    final class LayerSurface: NSView {
        private let host = (NSClassFromString("CALayerHost") as? CALayer.Type)?.init()
        private var context: UInt32 = 0
        private var page = CGSize.zero

        override var isFlipped: Bool {
            true
        }

        init() {
            super.init(frame: .zero)
            let root = CALayer()
            root.isGeometryFlipped = true
            layer = root
            wantsLayer = true
            if let host {
                host.anchorPoint = .zero
                host.isGeometryFlipped = true
                host.masksToBounds = true
                root.addSublayer(host)
            }
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            nil
        }

        override func hitTest(_: NSPoint) -> NSView? {
            nil
        }

        func show(_ context: UInt32, page: CGSize) {
            if context != self.context {
                self.context = context
                host?.setValue(NSNumber(value: context), forKey: "contextId")
            }
            if page != self.page {
                self.page = page
                place()
            }
        }

        override func layout() {
            super.layout()
            place()
        }

        /// Floors `bounds` to match the integer point size sent to the host and aligns the origin
        /// to backing pixels for crisp 1:1 rendering.
        private func place() {
            guard let host, page.width > 0, page.height > 0 else { return }
            let fit = min(bounds.width.rounded(.down) / page.width, bounds.height.rounded(.down) / page.height)
            guard fit > 0 else { return }
            let shown = CGSize(width: page.width * fit, height: page.height * fit)
            let centered = CGRect(origin: CGPoint(x: (bounds.width - shown.width) / 2, y: (bounds.height - shown.height) / 2), size: shown)
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            host.bounds = CGRect(origin: .zero, size: page)
            host.position = backingAlignedRect(centered, options: .alignAllEdgesNearest).origin
            host.transform = CATransform3DMakeScale(fit, fit, 1)
            CATransaction.commit()
        }
    }
#endif
