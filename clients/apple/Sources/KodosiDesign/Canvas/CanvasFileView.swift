import SwiftUI
#if os(macOS)
    import QuickLookUI
#else
    import QuickLook
#endif

/// A picture, PDF or Office file of a canvas, which Quick Look shows from a copy in the
/// temporary folder that goes when the view goes.
public struct CanvasFileView: View {
    let content: Data
    let name: String
    @State private var copy: CanvasCopy?

    public init(content: Data, name: String) {
        self.content = content
        self.name = name
    }

    public var body: some View {
        ZStack {
            if let copy {
                QuickLookView(url: copy.url)
            }
        }
        .task(id: content) { copy = try? CanvasCopy(content, name: name) }
    }
}

final class CanvasCopy {
    private static let folder = FileManager.default.temporaryDirectory.appending(path: "Kodosi Canvas", directoryHint: .isDirectory)

    let url: URL

    init(_ content: Data, name: String) throws {
        let folder = Self.folder.appending(path: UUID().uuidString, directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let safe = String(name.map { "/:\\".contains($0) ? "-" : $0 })
        url = folder.appending(path: safe.isEmpty || safe.hasPrefix(".") ? "item" + safe : safe)
        try content.write(to: url, options: .atomic)
    }

    deinit {
        try? FileManager.default.removeItem(at: url.deletingLastPathComponent())
    }
}

#if os(macOS)
    private struct QuickLookView: NSViewRepresentable {
        let url: URL

        func makeNSView(context _: Context) -> QLPreviewView {
            let view = QLPreviewView(frame: .zero, style: .normal) ?? QLPreviewView()
            view.shouldCloseWithWindow = false
            view.previewItem = url as NSURL
            return view
        }

        func updateNSView(_ view: QLPreviewView, context _: Context) {
            if (view.previewItem as? NSURL) as URL? != url {
                view.previewItem = url as NSURL
            }
        }

        static func dismantleNSView(_ view: QLPreviewView, coordinator _: ()) {
            view.close()
        }
    }

#else
    private struct QuickLookView: UIViewControllerRepresentable {
        let url: URL

        func makeUIViewController(context: Context) -> QLPreviewController {
            let controller = QLPreviewController()
            controller.dataSource = context.coordinator
            return controller
        }

        func updateUIViewController(_ controller: QLPreviewController, context: Context) {
            guard context.coordinator.url != url else { return }
            context.coordinator.url = url
            controller.reloadData()
        }

        func makeCoordinator() -> Source {
            Source(url: url)
        }

        final class Source: NSObject, QLPreviewControllerDataSource {
            var url: URL

            init(url: URL) {
                self.url = url
            }

            func numberOfPreviewItems(in _: QLPreviewController) -> Int {
                1
            }

            func previewController(_: QLPreviewController, previewItemAt _: Int) -> any QLPreviewItem {
                url as NSURL
            }
        }
    }
#endif
