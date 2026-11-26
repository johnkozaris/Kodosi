import KodosiBridge
import SwiftUI

/// Renders the local `CALayerHost` when available, with `VideoView` underneath as a
/// fallback still. While the layer is active, the video view matches the page's aspect ratio so a
/// previous-size still cannot peek out beside the layer.
struct BrowserPicture: View {
    let feed: BrowserFeed
    /// False while the view says something in place of the page, or the page is blank.
    let shows: Bool
    let resting: Bool

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
            VideoView(feed: feed, resting: resting)
                .aspectRatio(layerRatio ?? Self.ratio(of: area.size), contentMode: .fit)
                .frame(width: area.size.width, height: area.size.height)
        }
        .opacity(shows ? 1 : 0)
        #if os(macOS)
            if let layer = feed.window.layer, shows {
                BrowserLayerView(context: layer, page: feed.window.size)
            }
        #endif
    }
}
