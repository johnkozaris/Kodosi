import ImageIO
import KodosiBridge
import SwiftUI
import UniformTypeIdentifiers
#if os(macOS)
    import AppKit
#else
    import PhotosUI
    import UIKit
#endif

/// The bytes of a picture as the runtime takes them: PNG, JPEG, GIF and WebP as they are, and any
/// other picture that this device reads as PNG.
enum ChatPictureBytes {
    static func taken(_ data: Data) -> Data? {
        if ChatDraftPicture.isPicture(data) {
            return data
        }
        #if os(macOS)
            guard let tiff = NSImage(data: data)?.tiffRepresentation else { return nil }
            return NSBitmapImageRep(data: tiff)?.representation(using: .png, properties: [:])
        #else
            return UIImage(data: data)?.pngData()
        #endif
    }
}

/// A picture, or a file of one, dropped on the chat.
struct ChatPictureDrop: Transferable {
    let data: Data?

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .image) { file in
            try ChatPictureDrop(data: ChatPictureBytes.taken(Data(contentsOf: file.file)))
        }
        DataRepresentation(importedContentType: .image) { data in
            ChatPictureDrop(data: ChatPictureBytes.taken(data))
        }
    }
}

extension TerminalChat {
    /// The person has words to send, or pictures while no question of the agent waits, which only
    /// words answer.
    var unsent: Bool {
        typed || (!pictures.isEmpty && question == nil)
    }
}

/// The pictures of a message, one tile each.
struct ChatPictureTiles: View {
    @Environment(\.theme) private var theme
    let count: Int
    var size: CGFloat = 26

    var body: some View {
        HStack(spacing: 4) {
            ForEach(0 ..< count, id: \.self) { _ in
                Image(systemName: "photo").font(.system(size: (size * 0.48).rounded(), weight: .medium))
                    .foregroundStyle(theme.colors.inkMuted)
                    .frame(width: size, height: size)
                    .well(Radius.xs)
                    .accessibilityLabel(Text("Picture", bundle: .module))
            }
        }
    }
}

/// The pictures that go with the next message, each with a key that takes it out.
struct ChatDraftPictures: View {
    @Environment(\.theme) private var theme
    let chat: TerminalChat

    var body: some View {
        ScrollView(.horizontal) {
            HStack(spacing: 10) {
                ForEach(Array(chat.pictures.enumerated()), id: \.element.id) { index, picture in
                    ChatDraftPictureView(picture: picture)
                        .accessibilityIdentifier("chat.draft.picture.\(index)")
                        .overlay(alignment: .topTrailing) {
                            IconButton(title: "Remove", bundle: .module, symbol: "xmark",
                                       identifier: "chat.draft.picture.remove.\(index)", size: 20)
                            {
                                chat.remove(picture)
                            }
                            .raisedCapsule(fill: theme.colors.lifted, elevation: .lifted)
                            .offset(x: 7, y: -7)
                        }
                }
            }
            .padding(.top, 10).padding(.horizontal, 10).padding(.trailing, 6)
        }
        .scrollIndicators(.hidden)
    }
}

private struct ChatDraftPictureView: View {
    @Environment(\.theme) private var theme
    let picture: ChatDraftPicture
    @State private var thumbnail: Image?

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: Radius.sm, style: .continuous)
        Group {
            if let thumbnail {
                thumbnail.resizable().scaledToFill()
            } else {
                Image(systemName: "photo").foregroundStyle(theme.colors.inkFaint)
            }
        }
        .frame(width: 48, height: 48)
        .clipShape(shape)
        .overlay { shape.strokeBorder(theme.colors.hairline, lineWidth: 0.5) }
        .accessibilityElement()
        .accessibilityLabel(Text("Picture", bundle: .module))
        .task(id: picture.id) { thumbnail = Self.thumbnail(of: picture.data) }
    }

    private static func thumbnail(of data: Data) -> Image? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [
                  kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 144,
                  kCGImageSourceCreateThumbnailWithTransform: true,
              ] as CFDictionary)
        else { return nil }
        return Image(decorative: image, scale: 3)
    }
}

#if os(iOS)
    /// Picks pictures from the photo library for the next message.
    struct ChatPictureKey: View {
        @Environment(\.theme) private var theme
        let chat: TerminalChat
        @State private var picking = false
        @State private var picked: [PhotosPickerItem] = []

        var body: some View {
            Button { picking = true } label: {
                Image(systemName: "photo").font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(chat.canWrite ? theme.colors.ink : theme.colors.inkFaint)
                    .frame(width: 30, height: 30)
                    .background { BezelBackground(shape: Circle(), fill: theme.buttonFace(), bezel: .quiet) }
                    .touchTarget()
            }
            .buttonStyle(PressScaleStyle())
            .disabled(!chat.canWrite)
            .accessibilityLabel(Text("Add picture", bundle: .module))
            .accessibilityIdentifier("chat.picture.add")
            .photosPicker(isPresented: $picking, selection: $picked,
                          maxSelectionCount: max(1, ChatDraftPicture.most - chat.pictures.count), matching: .images)
            .onChange(of: picked) { _, items in
                guard !items.isEmpty else { return }
                picked = []
                Task {
                    for item in items {
                        let data = try? await item.loadTransferable(type: Data.self)
                        chat.attach(data.flatMap(ChatPictureBytes.taken))
                    }
                }
            }
        }
    }
#endif

extension View {
    /// A picture dropped on the chat goes with the next message.
    func takesPictures(_ chat: TerminalChat) -> some View {
        modifier(ChatPictureDropping(chat: chat))
    }
}

private struct ChatPictureDropping: ViewModifier {
    let chat: TerminalChat
    @State private var targeted = false

    func body(content: Content) -> some View {
        content
            .overlay { DropSlot(radius: Radius.lg, shown: targeted).padding(6) }
            .dropDestination(for: ChatPictureDrop.self) { pictures, _ in
                guard chat.canWrite else { return false }
                for picture in pictures {
                    chat.attach(picture.data)
                }
                return true
            } isTargeted: { targeted = $0 && chat.canWrite }
    }
}

#if os(macOS)
    /// Paste in the composer adds the pictures on the pasteboard to the next message: picture files,
    /// or a picture without words. Words paste into the draft as usual.
    struct ChatPicturePaste: ViewModifier {
        let chat: TerminalChat
        let focused: Bool
        @State private var monitor: Any?

        func body(content: Content) -> some View {
            content
                // The text field takes Paste itself, so a monitor sees the key first, while the
                // composer has the focus.
                .onChange(of: focused, initial: true) { _, focused in
                    stop()
                    if focused {
                        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [chat] event in
                            guard event.modifierFlags.intersection(.deviceIndependentFlagsMask) == .command,
                                  event.charactersIgnoringModifiers == "v", chat.canWrite,
                                  let pictures = Self.pictures(on: .general)
                            else { return event }
                            for picture in pictures {
                                chat.attach(picture)
                            }
                            return nil
                        }
                    }
                }
                .onDisappear(perform: stop)
        }

        private func stop() {
            if let monitor {
                NSEvent.removeMonitor(monitor)
            }
            monitor = nil
        }

        static func pictures(on board: NSPasteboard) -> [Data?]? {
            let files = board.readObjects(forClasses: [NSURL.self], options: [
                .urlReadingFileURLsOnly: true, .urlReadingContentsConformToTypes: [UTType.image.identifier],
            ]) as? [URL] ?? []
            if !files.isEmpty {
                return files.map { try? ChatPictureBytes.taken(Data(contentsOf: $0)) }
            }
            guard board.string(forType: .string) == nil else { return nil }
            let pictures = (board.pasteboardItems ?? []).compactMap { item -> Data?? in
                let kinds: [NSPasteboard.PasteboardType] = [
                    .png, .init(UTType.jpeg.identifier), .init(UTType.gif.identifier),
                    .init(UTType.webP.identifier), .tiff, .init(UTType.heic.identifier),
                ]
                guard let data = kinds.lazy.compactMap({ item.data(forType: $0) }).first else { return nil }
                return .some(ChatPictureBytes.taken(data))
            }
            return pictures.isEmpty ? nil : pictures
        }
    }
#endif
