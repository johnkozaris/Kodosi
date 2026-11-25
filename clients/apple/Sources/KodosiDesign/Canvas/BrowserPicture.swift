import KodosiBridge
import SwiftUI

/// Renders the local `CALayerHost` when available, with `VideoView` underneath as a
/// fallback still. While the layer is active, the video view matches the page's aspect ratio so a
/// previous-size still cannot peek out beside the layer.
struct BrowserPicture: View {
    @Environment(\.theme) private var theme
    let browser: CanvasBrowser
    let feed: BrowserFeed
    let blank: Bool

    /// Returns `nil` in video-only mode so the video view fits each picture itself, without lagging
    /// behind frame size changes by a SwiftUI layout pass.
    private var layerRatio: CGFloat? {
        feed.window.layer == nil ? nil : Self.ratio(of: feed.window.size)
    }

    private static func ratio(of size: CGSize) -> CGFloat? {
        size.width > 0 && size.height > 0 ? size.width / size.height : nil
    }

    var body: some View {
        GeometryReader { area in
            VideoView(feed: feed)
                .aspectRatio(layerRatio ?? Self.ratio(of: area.size), contentMode: .fit)
                .frame(width: area.size.width, height: area.size.height)
        }
        .opacity(feed.picture != .waiting && !blank ? 1 : 0)
        .grayscale(browser.state == .awake ? 0 : 1)
        .animation(theme.motion.fade, value: browser.state)
        #if os(macOS)
            if let layer = feed.window.layer, !blank {
                BrowserLayerView(context: layer, page: feed.window.size)
            }
        #endif
    }
}
