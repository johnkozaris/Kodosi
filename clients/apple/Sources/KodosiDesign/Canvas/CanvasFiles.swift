import CoreGraphics
import ImageIO
import KodosiBridge
import PDFKit
import SwiftUI

#if os(iOS)
    private let row: CGFloat = 40
#else
    private let row: CGFloat = 26
#endif

/// The files of a canvas over the file that it shows: the key of the cards with the count, and a
/// tab for each file. The chosen tab is a raised pill. On a pointer, a tab's glyph becomes the
/// key that takes the file off the canvas; `actions` is its menu.
public struct CanvasFilesBar<Actions: View>: View {
    @Environment(\.theme) private var theme
    @Namespace private var thumb
    let session: RuntimeSession
    let store: CanvasStore
    let chosen: String?
    let pick: (String) -> Void
    let remove: (CanvasItem) -> Void
    let actions: (CanvasItem) -> Actions
    @State private var hovered: String?

    public init(session: RuntimeSession, store: CanvasStore, chosen: String?, pick: @escaping (String) -> Void,
                remove: @escaping (CanvasItem) -> Void, @ViewBuilder actions: @escaping (CanvasItem) -> Actions)
    {
        self.session = session
        self.store = store
        self.chosen = chosen
        self.pick = pick
        self.remove = remove
        self.actions = actions
    }

    public var body: some View {
        let files = session.canvas?.files ?? []
        let shelf = store.showsShelf(session.id)
        HStack(spacing: 4) {
            Button {
                withAnimation(theme.motion.snappy) { store.showShelf(!shelf, in: session.id) }
            } label: {
                HStack(spacing: 5) {
                    Image(systemName: "square.grid.2x2").font(.system(size: 11, weight: .semibold))
                    Text(files.count, format: .number).monospacedDigit().contentTransition(.numericText())
                }
                .appTextStyle(.footnote).fontWeight(.medium)
                .foregroundStyle(shelf ? theme.colors.ink : theme.colors.inkMuted)
                .padding(.horizontal, 9).frame(height: row - 2)
                .background {
                    if shelf {
                        RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: .resting)
                            .matchedGeometryEffect(id: "thumb", in: thumb)
                    }
                }
                .contentShape(Capsule())
            }
            .buttonStyle(.plain)
            .help(String(localized: "All files", bundle: .module))
            .accessibilityAddTraits(shelf ? .isSelected : [])
            .accessibilityIdentifier("canvas.files.all")
            ScrollViewReader { reader in
                ScrollView(.horizontal) {
                    HStack(spacing: 2) {
                        ForEach(files) { file in
                            tab(file, selected: !shelf && file.key == chosen)
                                .id(file.key)
                                .transition(.scale(scale: 0.9).combined(with: .opacity))
                        }
                    }
                    .padding(.vertical, 1)
                }
                .scrollIndicators(.never)
                .onChange(of: chosen, initial: true) { _, key in
                    if let key {
                        withAnimation(theme.motion.snappy) { reader.scrollTo(key) }
                    }
                }
            }
        }
        .padding(.horizontal, 6).frame(height: row + 6)
        .animation(theme.motion.snappy, value: files.map(\.key))
        .animation(theme.motion.snappy, value: shelf ? nil : chosen)
    }

    private func tab(_ file: CanvasItem, selected: Bool) -> some View {
        let news = !selected && store.isNew(.item(file), in: session.id)
        let closing = hovered == file.key
        return Button { pick(file.key) } label: {
            HStack(spacing: 5) {
                Image(systemName: file.kind.symbol).font(.system(size: 10.5, weight: .semibold))
                    .foregroundStyle(selected ? theme.colors.accentStrong : theme.colors.inkFaint)
                    .frame(width: 14)
                    .opacity(closing ? 0 : 1)
                Text(file.name).lineLimit(1).truncationMode(.middle)
            }
            .appTextStyle(.footnote).fontWeight(.medium)
            .foregroundStyle(file.missing ? theme.colors.inkFaint : selected ? theme.colors.ink : theme.colors.inkMuted)
            .padding(.leading, 7).padding(.trailing, 10).frame(height: row - 2).frame(maxWidth: 200)
            .background {
                if selected {
                    RaisedBackground(shape: Capsule(), fill: theme.colors.raised, elevation: .resting)
                        .matchedGeometryEffect(id: "thumb", in: thumb)
                }
            }
            .overlay(alignment: .topTrailing) {
                if news {
                    BreathingDot(color: theme.colors.glowOrange, size: 6).offset(x: -2, y: 2)
                        .allowsHitTesting(false).transition(AnyTransition.pop)
                }
            }
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .help(file.missing ? String(localized: "\(file.key) is not there now", bundle: .module) : file.key)
        .accessibilityAddTraits(selected ? .isSelected : [])
        .accessibilityAction(named: Text("Remove from Canvas", bundle: .module)) { remove(file) }
        .accessibilityIdentifier("canvas.file.\(AccessibilityIdentifier.token(file.key))")
        .overlay(alignment: .leading) {
            if closing {
                RemoveKey(size: 16, identifier: "canvas.remove.\(AccessibilityIdentifier.token(file.key))") { remove(file) }
                    .padding(.leading, 6)
                    .transition(.opacity)
            }
        }
        .onHover { inside in
            withAnimation(theme.motion.hover) {
                if inside {
                    hovered = file.key
                } else if hovered == file.key {
                    hovered = nil
                }
            }
        }
        .contextMenu { actions(file) }
    }
}

/// The small round key that takes a file off the canvas, or closes a part of it.
struct RemoveKey: View {
    @Environment(\.theme) private var theme
    let size: CGFloat
    var title = String(localized: "Remove from Canvas", bundle: .module)
    let identifier: String
    let action: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            Image(systemName: "xmark").font(.system(size: size * 0.5, weight: .bold))
                .foregroundStyle(hovered ? theme.colors.ink : theme.colors.inkMuted)
                .frame(width: size, height: size)
                .background(theme.colors.ink.opacity(hovered ? 0.14 : 0.08), in: Circle())
                .contentShape(Circle())
        }
        .buttonStyle(PressScaleStyle(scale: 0.88))
        .onHover { hovered = $0 }
        .help(title)
        .accessibilityLabel(Text(title))
        .accessibilityIdentifier(identifier)
    }
}

/// All the files of a canvas as cards.
public struct CanvasShelf<Actions: View>: View {
    @Environment(\.theme) private var theme
    let session: RuntimeSession
    let store: CanvasStore
    let pick: (String) -> Void
    let remove: (CanvasItem) -> Void
    let actions: (CanvasItem) -> Actions

    public init(session: RuntimeSession, store: CanvasStore, pick: @escaping (String) -> Void,
                remove: @escaping (CanvasItem) -> Void, @ViewBuilder actions: @escaping (CanvasItem) -> Actions)
    {
        self.session = session
        self.store = store
        self.pick = pick
        self.remove = remove
        self.actions = actions
    }

    public var body: some View {
        let files = (session.canvas?.files ?? []).reversed()
        ScrollView {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 128, maximum: 172), spacing: 18)], spacing: 22) {
                ForEach(Array(files)) { file in
                    CanvasCard(file: file, content: store.stored(file, in: session.id), news: store.isNew(.item(file), in: session.id),
                               remove: { remove(file) }, action: { pick(file.key) })
                        .contextMenu { actions(file) }
                        .transition(.scale(scale: 0.94).combined(with: .opacity))
                }
            }
            .padding(.horizontal, 22).padding(.vertical, 24)
        }
        .animation(theme.motion.spring, value: files.map(\.key))
        .accessibilityIdentifier("canvas.shelf")
    }
}

/// A file as a card: a sheet with its picture, the first page of a PDF or the title of a page
/// when this device has the content, and else the tile of its kind.
struct CanvasCard: View {
    @Environment(\.theme) private var theme
    let file: CanvasItem
    let content: Data?
    let news: Bool
    let remove: () -> Void
    let action: () -> Void
    @State private var preview: CanvasPreview?
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            VStack(spacing: 10) {
                sheet
                    .frame(height: 104)
                    .offset(y: hovered ? -2 : 0)
                    .rotationEffect(.degrees(hovered ? -1.2 : 0), anchor: .bottom)
                VStack(spacing: 2) {
                    Text(file.name).appTextStyle(.footnote).fontWeight(.medium)
                        .foregroundStyle(file.missing ? theme.colors.inkFaint : theme.colors.ink)
                        .lineLimit(2).truncationMode(.middle).multilineTextAlignment(.center)
                    Text(file.missing ? String(localized: "Not there now", bundle: .module)
                        : "\(file.kind.label) · \(file.bytes.formatted(.byteCount(style: .file)))")
                        .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint).lineLimit(1)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityAction(named: Text("Remove from Canvas", bundle: .module), remove)
        .accessibilityIdentifier("canvas.card.\(AccessibilityIdentifier.token(file.key))")
        .overlay(alignment: .topLeading) {
            if hovered {
                RemoveKey(size: 20, identifier: "canvas.card.remove.\(AccessibilityIdentifier.token(file.key))", action: remove)
                    .background(theme.colors.surface, in: Circle())
                    .offset(x: -7, y: -7)
                    .transition(.opacity.combined(with: .scale(scale: 0.6)))
            }
        }
        .onHover { hovered = $0 }
        .animation(theme.motion.spring, value: hovered)
        .task(id: Revision(key: file.key, revision: file.revision, stored: content != nil)) {
            if let content {
                preview = await CanvasPreview.of(content, kind: file.kind)
            } else {
                preview = nil
            }
        }
    }

    private struct Revision: Equatable {
        let key: String
        let revision: UInt64
        let stored: Bool
    }

    @ViewBuilder
    private var sheet: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.md, style: .continuous)
        Color.clear.overlay {
            switch preview {
            case let .picture(image):
                Image(decorative: image, scale: 1).resizable().interpolation(.high).scaledToFill()
            case let .title(title):
                VStack(alignment: .leading, spacing: 6) {
                    Text(title).font(.system(size: 11, weight: .bold)).foregroundStyle(theme.colors.ink).lineLimit(2)
                    ForEach([0.92, 0.8, 0.86, 0.5], id: \.self) { part in
                        GeometryReader { geometry in
                            Capsule().fill(theme.colors.inkFaint.opacity(0.35)).frame(width: geometry.size.width * part, height: 4)
                        }
                        .frame(height: 4)
                    }
                }
                .padding(12).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            case nil:
                IconTile(symbol: file.kind.symbol, tint: file.missing ? theme.colors.inkFaint : TileTint.amber, size: 34)
            }
        }
        .clipShape(shape)
        .background {
            if file.missing {
                shape.strokeBorder(theme.colors.hairline, style: StrokeStyle(lineWidth: 1.5, dash: [5, 4]))
            } else {
                RaisedBackground(shape: shape, fill: theme.colors.lifted, elevation: hovered ? .lifted : .resting)
            }
        }
        .overlay(alignment: .topTrailing) {
            if news {
                BreathingDot(color: theme.colors.glowOrange, size: 7).offset(x: 3, y: -3).transition(AnyTransition.pop)
            }
        }
    }
}

/// What a card shows of a file whose content this device has.
enum CanvasPreview: @unchecked Sendable {
    case picture(CGImage)
    case title(String)

    @concurrent
    static func of(_ content: Data, kind: CanvasItem.Kind) async -> CanvasPreview? {
        switch kind {
        case .picture:
            guard let source = CGImageSourceCreateWithData(content as CFData, nil),
                  let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                      kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 360,
                      kCGImageSourceCreateThumbnailWithTransform: true,
                  ] as CFDictionary)
            else { return nil }
            return .picture(image)
        case .pdf:
            guard let page = PDFDocument(data: content)?.page(at: 0) else { return nil }
            let bounds = page.bounds(for: .mediaBox)
            let scale = 360 / max(bounds.width, bounds.height, 1)
            #if os(macOS)
                let image = page.thumbnail(of: CGSize(width: bounds.width * scale, height: bounds.height * scale), for: .mediaBox)
                return image.cgImage(forProposedRect: nil, context: nil, hints: nil).map(CanvasPreview.picture)
            #else
                return page.thumbnail(of: CGSize(width: bounds.width * scale, height: bounds.height * scale), for: .mediaBox)
                    .cgImage.map(CanvasPreview.picture)
            #endif
        case .page:
            let head = content.prefix(64 * 1024)
            let text = (0 ... 3).lazy.compactMap { String(bytes: head.dropLast($0), encoding: .utf8) }.first
            return text.flatMap(title(in:)).map(CanvasPreview.title)
        case .office:
            return nil
        }
    }

    private static func title(in page: String) -> String? {
        for tag in ["title", "h1"] {
            if let open = page.range(of: "<\(tag)", options: .caseInsensitive),
               let start = page[open.upperBound...].firstIndex(of: ">"),
               let close = page[start...].range(of: "</\(tag)>", options: .caseInsensitive)
            {
                let words = page[page.index(after: start) ..< close.lowerBound]
                    .replacingOccurrences(of: "<[^>]+>", with: "", options: .regularExpression)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                if !words.isEmpty {
                    return String(words.prefix(80))
                }
            }
        }
        return nil
    }
}
