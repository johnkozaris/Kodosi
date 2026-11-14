import AVFoundation
import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit
import Vision
import VisionKit

struct CodeScan {
    let hint: LocalizedStringResource
    let other: LocalizedStringResource
    let identifier: String
    let accepts: (String) -> String?
    let found: (String) -> Void
}

struct CodeScanSheet: View {
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    let title: LocalizedStringResource
    let scan: CodeScan

    var body: some View {
        NavigationStack {
            CodeScanView(scan: scan)
                .padding(.horizontal, 16)
                .padding(.bottom, 16)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(theme.colors.surface)
                .navigationTitle(Text(title))
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) {
                        IconButton(title: "Close", symbol: "xmark", identifier: "\(scan.identifier).close", size: 32) { dismiss() }
                    }
                    .sharedBackgroundVisibility(.hidden)
                }
        }
        .presentationDetents([.large])
        .kodosiSheet()
    }
}

struct CodeScanView: View {
    @Environment(\.theme) private var theme
    @Environment(\.openURL) private var openURL
    @State private var camera = Camera.asking
    @State private var otherCode = false
    let scan: CodeScan

    var body: some View {
        VStack(spacing: 16) {
            switch camera {
            case .denied: blocked
            case .unavailable: unavailable
            case .asking, .ready:
                viewfinder
                hint
            }
        }
        .animation(theme.motion.fade, value: otherCode)
        .animation(theme.motion.fade, value: camera)
        .onChange(of: otherCode) { _, other in
            if other {
                AccessibilityNotification.Announcement(String(localized: scan.other)).post()
            }
        }
        .task {
            switch AVCaptureDevice.authorizationStatus(for: .video) {
            case .authorized: camera = .ready
            case .notDetermined: camera = await AVCaptureDevice.requestAccess(for: .video) ? .ready : .denied
            default: camera = .denied
            }
        }
    }

    private var viewfinder: some View {
        ZStack {
            theme.colors.terminal
            if camera == .ready {
                CodeScanner(accepts: scan.accepts, found: scan.found) {
                    otherCode = true
                } failed: {
                    camera = AVCaptureDevice.authorizationStatus(for: .video) == .authorized ? .unavailable : .denied
                }
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: Radius.xl, style: .continuous))
        .accessibilityHidden(true)
    }

    @ViewBuilder
    private var hint: some View {
        if otherCode {
            ErrorNote(message: String(localized: scan.other))
                .accessibilityIdentifier("\(scan.identifier).other")
        } else {
            Text(scan.hint)
                .appTextStyle(.callout)
                .foregroundStyle(theme.colors.inkMuted)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
                .frame(minHeight: 40)
        }
    }

    private var blocked: some View {
        VStack(spacing: 16) {
            Text("Turn on the camera for Kodosi in Settings")
                .appTextStyle(.body)
                .foregroundStyle(theme.colors.ink)
                .multilineTextAlignment(.center)
            Button("Settings") {
                if let settings = URL(string: UIApplication.openSettingsURLString) {
                    openURL(settings)
                }
            }
            .buttonStyle(.kodosi(.primary, size: .large))
            .accessibilityIdentifier("\(scan.identifier).settings")
        }
        .padding(16)
        .frame(maxWidth: 400, maxHeight: .infinity)
    }

    private var unavailable: some View {
        Text("The code scanner is not available.")
            .appTextStyle(.body)
            .foregroundStyle(theme.colors.ink)
            .multilineTextAlignment(.center)
            .padding(16)
            .frame(maxWidth: 400, maxHeight: .infinity)
            .accessibilityIdentifier("\(scan.identifier).unavailable")
    }

    private enum Camera {
        case asking, ready, denied, unavailable
    }
}

private struct CodeScanner: UIViewControllerRepresentable {
    let accepts: (String) -> String?
    let found: (String) -> Void
    let other: () -> Void
    let failed: () -> Void

    func makeUIViewController(context: Context) -> DataScannerViewController {
        let scanner = DataScannerViewController(recognizedDataTypes: [.barcode(symbologies: [.qr])], isHighlightingEnabled: true)
        scanner.delegate = context.coordinator
        return scanner
    }

    func updateUIViewController(_ scanner: DataScannerViewController, context: Context) {
        context.coordinator.scanner = self
        guard !scanner.isScanning, !context.coordinator.stopped else { return }
        do {
            try scanner.startScanning()
        } catch {
            Task { failed() }
        }
    }

    static func dismantleUIViewController(_ scanner: DataScannerViewController, coordinator _: Coordinator) {
        scanner.stopScanning()
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(scanner: self)
    }

    final class Coordinator: NSObject, DataScannerViewControllerDelegate {
        var scanner: CodeScanner
        var stopped = false

        init(scanner: CodeScanner) {
            self.scanner = scanner
        }

        func dataScanner(_ dataScanner: DataScannerViewController, didAdd addedItems: [RecognizedItem], allItems _: [RecognizedItem]) {
            for case let .barcode(code) in addedItems {
                guard let payload = code.payloadStringValue else { continue }
                guard let accepted = scanner.accepts(payload) else {
                    scanner.other()
                    continue
                }
                stopped = true
                dataScanner.stopScanning()
                scanner.found(accepted)
                return
            }
        }

        func dataScanner(_: DataScannerViewController, becameUnavailableWithError _: DataScannerViewController.ScanningUnavailable) {
            scanner.failed()
        }
    }
}
