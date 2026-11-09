import KodosiBridge
import SwiftUI

/// The mark of a file: a sheet with a turned corner, lit from the top left like a tile, in the
/// color that people know for its kind and with its letter or sign, as a Word file is blue
/// with a W.
public struct FileIcon: View {
    @Environment(\.theme) private var theme
    let look: FileLook
    let size: CGFloat
    var missing = false

    public init(name: String, kind: CanvasItem.Kind, size: CGFloat, missing: Bool = false) {
        look = FileLook(name: name, kind: kind)
        self.size = size
        self.missing = missing
    }

    public var body: some View {
        let color = missing ? theme.colors.inkFaint : look.color
        let fold = size * 0.28
        let sheet = FileSheet(fold: fold, radius: size * 0.12)
        mark
            .foregroundStyle(.white)
            .padding(.top, fold * 0.5)
            .frame(width: size * 0.78, height: size)
            .background {
                sheet.fill(LinearGradient(
                    colors: [color.mix(with: .white, by: 0.22), color, color.mix(with: .black, by: 0.2)],
                    startPoint: .topLeading, endPoint: .bottomTrailing,
                ))
            }
            .overlay(alignment: .topTrailing) {
                FileFold(radius: size * 0.05).fill(color.mix(with: .white, by: 0.45)).frame(width: fold, height: fold)
            }
            .overlay {
                sheet.stroke(LinearGradient(colors: [.white.opacity(0.4), .clear, .black.opacity(0.18)],
                                            startPoint: .topLeading, endPoint: .bottomTrailing), lineWidth: 0.75)
            }
            .shadow(color: theme.colors.shadow.opacity(theme.isDark ? 0.4 : 0.18), radius: size * 0.06, x: 0.5, y: 1)
            .frame(width: size, height: size)
            .accessibilityHidden(true)
    }

    @ViewBuilder
    private var mark: some View {
        switch look.mark {
        case let .letters(letters):
            Text(verbatim: letters)
                .font(.system(size: size * (letters.count > 1 ? 0.27 : 0.42), weight: .heavy, design: .rounded))
                .minimumScaleFactor(0.5).lineLimit(1)
        case let .symbol(symbol):
            Image(systemName: symbol).font(.system(size: size * 0.34, weight: .bold))
        }
    }
}

/// The color and the sign of a kind of file, by its extension, and else by what it shows as.
struct FileLook: Equatable {
    enum Mark: Equatable {
        case letters(String)
        case symbol(String)
    }

    let color: Color
    let mark: Mark

    private static let word = FileLook(0x2F64C0, .letters("W"))
    private static let excel = FileLook(0x1F8A52, .letters("X"))
    private static let powerPoint = FileLook(0xD2552F, .letters("P"))
    private static let pdf = FileLook(0xD8463E, .letters("PDF"))
    private static let markup = FileLook(0xE06A3B, .symbol("chevron.left.forwardslash.chevron.right"))
    private static let text = FileLook(0x7A6F68, .symbol("text.alignleft"))
    private static let table = FileLook(0x3F9E6B, .symbol("tablecells"))

    private static let byEnding: [String: FileLook] = [
        "docx": word, "docm": word, "doc": word, "dotx": word,
        "xlsx": excel, "xlsm": excel, "xls": excel, "xltx": excel,
        "pptx": powerPoint, "pptm": powerPoint, "ppt": powerPoint, "potx": powerPoint,
        "pdf": pdf,
        "md": FileLook(0x5E6B85, .symbol("number")), "markdown": FileLook(0x5E6B85, .symbol("number")),
        "json": FileLook(0xD9A441, .symbol("curlybraces")),
        "yaml": FileLook(0xC2566E, .symbol("list.bullet.indent")), "yml": FileLook(0xC2566E, .symbol("list.bullet.indent")),
        "toml": FileLook(0x8C7BD1, .symbol("slider.horizontal.3")),
        "html": markup, "htm": markup,
        "xml": FileLook(0x4F9BB0, .symbol("chevron.left.forwardslash.chevron.right")),
        "csv": table, "tsv": table,
        "diff": FileLook(0x5B82C9, .symbol("plus.forwardslash.minus")), "patch": FileLook(0x5B82C9, .symbol("plus.forwardslash.minus")),
        "txt": text, "log": text,
    ]

    init(name: String, kind: CanvasItem.Kind) {
        if let look = Self.byEnding[(name as NSString).pathExtension.lowercased()] {
            self = look
            return
        }
        self = switch kind {
        case .page: FileLook(0x7A6F68, .symbol("doc.richtext"))
        case .picture: FileLook(0x4F9BB0, .symbol("photo"))
        case .pdf: Self.pdf
        case .document: FileLook(0x2F64C0, .symbol("doc.text"))
        case .sheet: Self.table
        case .presentation: FileLook(0xD2552F, .symbol("play.rectangle"))
        }
    }

    private init(_ color: UInt32, _ mark: Mark) {
        self.color = hex(color)
        self.mark = mark
    }
}

/// A sheet whose top trailing corner is cut for the turned corner.
private struct FileSheet: Shape {
    let fold: CGFloat
    let radius: CGFloat

    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.minX + radius, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX - fold, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY + fold))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY - radius))
        path.addArc(tangent1End: CGPoint(x: rect.maxX, y: rect.maxY), tangent2End: CGPoint(x: rect.maxX - radius, y: rect.maxY),
                    radius: radius)
        path.addLine(to: CGPoint(x: rect.minX + radius, y: rect.maxY))
        path.addArc(tangent1End: CGPoint(x: rect.minX, y: rect.maxY), tangent2End: CGPoint(x: rect.minX, y: rect.maxY - radius),
                    radius: radius)
        path.addLine(to: CGPoint(x: rect.minX, y: rect.minY + radius))
        path.addArc(tangent1End: CGPoint(x: rect.minX, y: rect.minY), tangent2End: CGPoint(x: rect.minX + radius, y: rect.minY),
                    radius: radius)
        path.closeSubpath()
        return path
    }
}

/// The turned corner: a triangle under the cut, with a soft inner corner.
private struct FileFold: Shape {
    let radius: CGFloat

    func path(in rect: CGRect) -> Path {
        var path = Path()
        path.move(to: CGPoint(x: rect.minX, y: rect.minY))
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX + radius, y: rect.maxY))
        path.addArc(tangent1End: CGPoint(x: rect.minX, y: rect.maxY), tangent2End: CGPoint(x: rect.minX, y: rect.maxY - radius),
                    radius: radius)
        path.closeSubpath()
        return path
    }
}
