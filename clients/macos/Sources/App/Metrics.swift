import CoreGraphics

enum Metrics {
    static let sidebarWidth: CGFloat = 252
    static let dosiWidth: CGFloat = 420
    static let sidebarRailWidth: CGFloat = 78
    static let frameInset: CGFloat = 8
    static let headerHeight: CGFloat = 54
    static let titleBarHeight: CGFloat = 36
}

extension Comparable {
    func clamped(to range: ClosedRange<Self>) -> Self {
        min(max(self, range.lowerBound), range.upperBound)
    }
}
