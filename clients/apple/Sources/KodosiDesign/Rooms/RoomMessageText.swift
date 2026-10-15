import Foundation
import SwiftUI

public struct RoomMessageText: View {
    @Environment(\.theme) private var theme
    let text: String
    let targets: [RoomMention]

    public init(text: String, targets: [RoomMention]) {
        self.text = text
        self.targets = targets
    }

    public var body: some View {
        let segments = RoomMentions.segments(text, targets: targets)
        if segments.allSatisfy({
            if case .text = $0 {
                true
            } else {
                false
            }
        }) {
            Text(Self.formatted(text))
        } else {
            segments.reduce(Text(verbatim: "")) { result, segment in
                switch segment {
                case let .text(value): Text("\(result)\(Text(Self.formatted(value)))")
                case let .mention(target): Text("\(result)\(chip(target))")
                }
            }
            .textRenderer(MentionRenderer())
        }
    }

    public static func formatted(_ text: String) -> AttributedString {
        let options = AttributedString.MarkdownParsingOptions(interpretedSyntax: .inlineOnlyPreservingWhitespace)
        var value = (try? AttributedString(markdown: text, options: options)) ?? AttributedString(text)
        guard let detector = linkDetector else { return value }
        let plain = String(value.characters)
        for match in detector.matches(in: plain, range: NSRange(plain.startIndex..., in: plain)) {
            guard let url = match.url, ["http", "https"].contains(url.scheme?.lowercased() ?? ""),
                  let range = Range(match.range, in: value), value[range].link == nil
            else { continue }
            value[range].link = url
        }
        return value
    }

    private static let linkDetector = try? NSDataDetector(types: NSTextCheckingResult.CheckingType.link.rawValue)

    private func chip(_ target: RoomMention) -> Text {
        let color = color(target)
        var label = AttributedString("@\(target.name)")
        label.swiftUI.foregroundColor = color
        #if os(iOS)
            label.swiftUI.font = .system(.body, weight: .semibold)
        #else
            label.swiftUI.font = .system(size: 13, weight: .semibold)
        #endif
        if let link = target.link {
            label.link = link
        }
        return Text(label).customAttribute(MentionChip(color: color))
    }

    private func color(_ target: RoomMention) -> Color {
        switch target.kind {
        case let .person(isSelf):
            isSelf ? theme.colors.accentStrong : PersonTint.color(for: target.id).mix(with: theme.colors.ink, by: 0.25)
        case let .terminal(kind):
            kind.isAgent ? kind.tint.mix(with: theme.colors.ink, by: 0.2) : theme.colors.accentStrong
        }
    }
}

private struct MentionChip: TextAttribute {
    let color: Color
}

private struct MentionRenderer: TextRenderer {
    func draw(layout: Text.Layout, in context: inout GraphicsContext) {
        for line in layout {
            for run in line {
                if let chip = run[MentionChip.self] {
                    let rect = run.typographicBounds.rect.insetBy(dx: -3, dy: 0)
                    context.fill(
                        RoundedRectangle(cornerRadius: 5, style: .continuous).path(in: rect),
                        with: .color(chip.color.opacity(0.18))
                    )
                }
                context.draw(run)
            }
        }
    }
}
