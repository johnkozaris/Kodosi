import KodosiBridge
import SwiftUI

public struct CanvasItemView: View {
    @Environment(\.theme) private var theme
    @AppStorage(DocumentViewers.storageKey) private var viewers = DocumentViewers()
    let session: RuntimeSession
    let shown: CanvasShown
    let store: CanvasStore
    let frames: CanvasFrames
    let pages: CanvasDocumentPages
    var typing = false
    var ask: CanvasAsk?
    @State private var content: Content?
    @State private var failure: String?

    private struct Content: Equatable {
        let key: String
        let revision: UInt64
        let data: Data
    }

    public init(session: RuntimeSession, shown: CanvasShown, store: CanvasStore, frames: CanvasFrames, pages: CanvasDocumentPages,
                typing: Bool = false, ask: CanvasAsk? = nil)
    {
        self.session = session
        self.shown = shown
        self.store = store
        self.frames = frames
        self.pages = pages
        self.typing = typing
        self.ask = ask
    }

    public var body: some View {
        switch shown {
        case let .browser(browser):
            CanvasBrowserView(browser: browser, feed: store.feed(session.id), typing: typing)
        case let .item(item):
            if let view = item.view, !item.missing, viewers.shows(view) {
                CanvasDocumentView(session: session.id, item: item, view: view, store: store, pages: pages,
                                   otherComputer: session.otherComputer)
            } else {
                file(item)
                    .task(id: Revision(key: item.key, revision: item.revision, missing: item.missing)) { await load(item) }
            }
        }
    }

    private struct Revision: Equatable {
        let key: String
        let revision: UInt64
        let missing: Bool
    }

    @ViewBuilder
    private func file(_ item: CanvasItem) -> some View {
        if let content, content.key == item.key {
            switch item.kind {
            case .page:
                CanvasPageView(key: item.key, frames: frames, content: content.data, otherComputer: session.otherComputer, ask: ask)
            case .picture, .pdf, .document, .sheet, .presentation:
                CanvasFileView(content: content.data, name: item.name, revision: content.revision).transition(.opacity)
            }
        } else if item.missing {
            CanvasMissing(item: item)
        } else if let failure {
            TerminalStateCard(symbol: "exclamationmark.triangle", title: String(localized: "This did not load", bundle: .module),
                              message: failure, working: false)
            {
                Button { Task { await load(item) } } label: { Text("Try again", bundle: .module) }.buttonStyle(.kodosi(.secondary))
            }
        } else {
            CanvasLoading(kind: item.kind)
        }
    }

    private func load(_ item: CanvasItem) async {
        guard !item.missing else { return }
        if content?.key != item.key {
            content = nil
        }
        failure = nil
        do {
            let (revision, data) = try await store.content(of: item, in: session.id)
            guard !Task.isCancelled else { return }
            withAnimation(theme.motion.fade) { content = Content(key: item.key, revision: revision, data: data) }
        } catch is CancellationError {
            return
        } catch {
            failure = error.localizedDescription
        }
    }
}

struct CanvasLoading: View {
    @Environment(\.theme) private var theme
    let kind: CanvasItem.Kind

    var body: some View {
        Group {
            if kind == .page {
                VStack(alignment: .leading, spacing: 11) {
                    bar(0.52, 17)
                    bar(0.3, 9).padding(.bottom, 12)
                    bar(0.94, 8)
                    bar(0.88, 8)
                    bar(0.7, 8).padding(.bottom, 8)
                    RoundedRectangle(cornerRadius: Radius.md, style: .continuous).fill(theme.colors.well).frame(height: 84)
                        .padding(.bottom, 8)
                    bar(0.9, 8)
                    bar(0.6, 8)
                }
                .padding(.horizontal, 32).padding(.top, 34)
                .frame(maxWidth: 660, maxHeight: .infinity, alignment: .topLeading)
            } else {
                RoundedRectangle(cornerRadius: Radius.sm, style: .continuous).fill(theme.colors.well)
                    .aspectRatio(kind == .picture ? 4 / 3 : 3 / 4, contentMode: .fit)
                    .overlay {
                        Image(systemName: kind.symbol)
                            .font(.system(size: 26, weight: .medium)).foregroundStyle(theme.colors.inkFaint.opacity(0.6))
                    }
                    .padding(36)
            }
        }
        .shimmer()
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityElement()
        .accessibilityLabel(Text("Loading", bundle: .module))
    }

    private func bar(_ part: CGFloat, _ height: CGFloat) -> some View {
        GeometryReader { geometry in
            Capsule().fill(theme.colors.well).frame(width: geometry.size.width * part, height: height)
        }
        .frame(height: height)
    }
}

struct CanvasMissing: View {
    @Environment(\.theme) private var theme
    let item: CanvasItem

    var body: some View {
        VStack(spacing: 10) {
            FileIcon(name: item.name, kind: item.kind, size: 34, missing: true)
            Text(item.name).appTextStyle(.footnote).fontWeight(.medium).foregroundStyle(theme.colors.inkMuted)
                .lineLimit(1).truncationMode(.middle)
        }
        .padding(.horizontal, 20)
        .frame(width: 240, height: 170)
        .background {
            RoundedRectangle(cornerRadius: Radius.xl, style: .continuous)
                .strokeBorder(theme.colors.hairline, style: StrokeStyle(lineWidth: 1.5, dash: [6, 5]))
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .help(String(localized: "\(item.key) is missing", bundle: .module))
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("canvas.missing")
    }
}
