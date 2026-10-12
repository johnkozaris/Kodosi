import Foundation
import UIKit

enum ThisDevice {
    static var isPad: Bool {
        UIDevice.current.userInterfaceIdiom == .pad
    }

    static var name: String {
        isPad ? String(localized: "iPad") : String(localized: "iPhone")
    }

    static var title: String {
        isPad ? String(localized: "This iPad") : String(localized: "This iPhone")
    }

    static var symbol: String {
        isPad ? "ipad" : "iphone"
    }
}
