import Foundation
import UIKit

enum RuntimeEnvironment {
    static let deviceLabelKey = "KODOSI_DEVICE_LABEL"

    static func install() {
        setenv(deviceLabelKey, UIDevice.current.model, 1)
    }
}
