import AVFoundation
import KodosiBridge
import KodosiDesign
import SwiftUI
import UIKit
import Vision
import VisionKit

struct InviteScanSheet: View {
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    @State private var camera: Bool?
    @State private var otherCode = false
    let found: (String) -> Void

    var body: some View {
        NavigationStack {
            VStack(spacing: 16) {
                if camera == false {
                    blocked
                } else {
                    viewfinder
                    hint
                }
            }
            .padding(.horizontal, 16)
            .padding(.bottom, 16)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(theme.colors.surface)
            .navigationTitle("Scan their invite")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    IconButton(title: "Close", symbol: "xmark", identifier: "people.scan.close", size: 32) { dismiss() }
                }
                .sharedBackgroundVisibility(.hidden)
            }
        }
        .presentationDetents([.large])
        .kodosiSheet()
        .animation(theme.motion.fade, value: otherCode)
        .animation(theme.motion.fade, value: camera)
        .onChange(of: otherCode) { _, other in
            if other {
                AccessibilityNotification.Announcement(String(localized: "This code is not a Kodosi invite.")).post()
            }
        }
        .task {
            switch AVCaptureDevice.authorizationStatus(for: .video) {
            case .authorized: camera = true
            case .notDetermined: camera = await AVCaptureDevice.requestAccess(for: .video)
            default: camera = false
            }
        }
    }

    private var viewfinder: some View {
        ZStack {
            theme.colors.terminal
            if camera == true {
                InviteScanner(found: found) {
                    otherCode = true
                } failed: {
                    camera = false
                }
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: Radius.xl, style: .continuous))
        .accessibilityHidden(true)
    }

    @ViewBuilder
    private var hint: some View {
        if otherCode {
            ErrorNote(message: String(localized: "This code is not a Kodosi invite."))
                .accessibilityIdentifier("people.scan.other")
        } else {
            Text("Point the camera at the code of their invite.")
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
            .accessibilityIdentifier("people.scan.settings")
        }
        .padding(16)
        .frame(maxWidth: 400, maxHeight: .infinity)
    }
}

private struct InviteScanner: UIViewControllerRepresentable {
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
        var scanner: InviteScanner
        var stopped = false

        init(scanner: InviteScanner) {
            self.scanner = scanner
        }

        func dataScanner(_ dataScanner: DataScannerViewController, didAdd addedItems: [RecognizedItem], allItems _: [RecognizedItem]) {
            for case let .barcode(code) in addedItems {
                guard let payload = code.payloadStringValue else { continue }
                guard let invite = PeopleStore.invite(in: payload) else {
                    scanner.other()
                    continue
                }
                stopped = true
                dataScanner.stopScanning()
                scanner.found(invite)
                return
            }
        }

        func dataScanner(_: DataScannerViewController, becameUnavailableWithError _: DataScannerViewController.ScanningUnavailable) {
            scanner.failed()
        }
    }
}
