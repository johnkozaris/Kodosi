import QuartzCore
import SwiftUI
#if os(macOS)
    import AppKit
#else
    import UIKit
#endif

struct LayerArt<Setup: Hashable>: View {
    let setup: Setup
    let draw: @MainActor (Setup, LayerCanvas) -> Void

    var body: some View {
        LayerHost(setup: setup, draw: draw)
            .allowsHitTesting(false)
            .accessibilityHidden(true)
    }
}

@MainActor
struct LayerCanvas {
    let layer: CALayer
    let size: CGSize
    let scale: CGFloat
    let environment: EnvironmentValues

    func color(_ color: Color) -> CGColor {
        color.resolve(in: environment).cgColor
    }

    func place(_ sublayer: CALayer, _ frame: CGRect, radius: CGFloat = 0) {
        sublayer.frame = frame
        sublayer.cornerRadius = radius
        sublayer.cornerCurve = .continuous
        sublayer.contentsScale = scale
        layer.addSublayer(sublayer)
    }
}

func loopPhase(_ period: TimeInterval) -> TimeInterval {
    Date.now.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: period)
}

extension CAAnimation {
    func looping(period: TimeInterval) -> Self {
        duration = period
        repeatCount = .infinity
        isRemovedOnCompletion = false
        timeOffset = loopPhase(period)
        return self
    }
}

#if os(macOS)
    private struct LayerHost<Setup: Hashable>: NSViewRepresentable {
        let setup: Setup
        let draw: @MainActor (Setup, LayerCanvas) -> Void

        func makeNSView(context _: Context) -> LayerHostView {
            LayerHostView()
        }

        func updateNSView(_ view: LayerHostView, context: Context) {
            let setup = setup
            let draw = draw
            view.update(setup: setup, environment: context.environment) { draw(setup, $0) }
        }
    }

    final class LayerHostView: NSView {
        let content = CALayer()
        private var setup: AnyHashable?
        private var environment = EnvironmentValues()
        private var drawing: (@MainActor (LayerCanvas) -> Void)?
        private var drawn: CGSize?

        override init(frame: NSRect) {
            super.init(frame: frame)
            wantsLayer = true
            layer?.addSublayer(content)
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            fatalError("init(coder:) is not available")
        }

        override var isFlipped: Bool {
            true
        }

        override func hitTest(_: NSPoint) -> NSView? {
            nil
        }

        func update(setup: AnyHashable, environment: EnvironmentValues, draw: @escaping @MainActor (LayerCanvas) -> Void) {
            self.environment = environment
            drawing = draw
            guard setup != self.setup else { return }
            self.setup = setup
            redraw()
        }

        override func layout() {
            super.layout()
            if drawn != bounds.size {
                redraw()
            }
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            redraw()
        }

        override func viewDidChangeBackingProperties() {
            super.viewDidChangeBackingProperties()
            redraw()
        }

        private func redraw() {
            guard let window else {
                content.sublayers = nil
                drawn = nil
                return
            }
            guard let drawing, bounds.width > 0, bounds.height > 0 else { return }
            drawn = bounds.size
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            content.frame = bounds
            content.sublayers = nil
            content.removeAllAnimations()
            content.masksToBounds = false
            content.cornerRadius = 0
            drawing(LayerCanvas(layer: content, size: bounds.size, scale: window.backingScaleFactor, environment: environment))
            CATransaction.commit()
        }
    }
#else
    private struct LayerHost<Setup: Hashable>: UIViewRepresentable {
        let setup: Setup
        let draw: @MainActor (Setup, LayerCanvas) -> Void

        func makeUIView(context _: Context) -> LayerHostView {
            LayerHostView()
        }

        func updateUIView(_ view: LayerHostView, context: Context) {
            let setup = setup
            let draw = draw
            view.update(setup: setup, environment: context.environment) { draw(setup, $0) }
        }
    }

    final class LayerHostView: UIView {
        let content = CALayer()
        private var setup: AnyHashable?
        private var environment = EnvironmentValues()
        private var drawing: (@MainActor (LayerCanvas) -> Void)?
        private var drawn: CGSize?

        override init(frame: CGRect) {
            super.init(frame: frame)
            isUserInteractionEnabled = false
            layer.addSublayer(content)
            registerForTraitChanges([UITraitDisplayScale.self]) { (view: LayerHostView, _: UITraitCollection) in
                view.redraw()
            }
        }

        @available(*, unavailable)
        required init?(coder _: NSCoder) {
            fatalError("init(coder:) is not available")
        }

        func update(setup: AnyHashable, environment: EnvironmentValues, draw: @escaping @MainActor (LayerCanvas) -> Void) {
            self.environment = environment
            drawing = draw
            guard setup != self.setup else { return }
            self.setup = setup
            redraw()
        }

        override func layoutSubviews() {
            super.layoutSubviews()
            if drawn != bounds.size {
                redraw()
            }
        }

        override func didMoveToWindow() {
            super.didMoveToWindow()
            redraw()
        }

        private func redraw() {
            guard window != nil else {
                content.sublayers = nil
                drawn = nil
                return
            }
            guard let drawing, bounds.width > 0, bounds.height > 0 else { return }
            drawn = bounds.size
            CATransaction.begin()
            CATransaction.setDisableActions(true)
            content.frame = bounds
            content.sublayers = nil
            content.removeAllAnimations()
            content.masksToBounds = false
            content.cornerRadius = 0
            drawing(LayerCanvas(layer: content, size: bounds.size, scale: traitCollection.displayScale, environment: environment))
            CATransaction.commit()
        }
    }
#endif
