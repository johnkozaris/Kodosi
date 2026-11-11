import Foundation
import KodosiBridge

public struct DocumentViewers: Equatable, Sendable {
    public static let storageKey = "documentViewers"

    public var documents = true
    public var documentPages = true
    public var darkDocuments = false
    public var sheets = true
    public var darkSheets = true
    public var presentations = true
    public var pdfs = true
    public var darkPdfs = false
    public var texts = true

    public init() {}

    public func shows(_ view: CanvasItem.View) -> Bool {
        switch view {
        case .docx: documents
        case .xlsx: sheets
        case .pptx: presentations
        case .pdf: pdfs
        case .text: texts
        }
    }

    func preferences(for view: CanvasItem.View) -> [String: Bool] {
        switch view {
        case .docx: ["pages": documentPages, "dark": darkDocuments]
        case .xlsx: ["dark": darkSheets]
        case .pptx: [:]
        case .pdf: ["dark": darkPdfs]
        case .text: [:]
        }
    }
}

/// Written out, because a `RawRepresentable` type with `String` raw values would otherwise code
/// itself as its raw value, which is this JSON.
extension DocumentViewers: Codable {
    private enum CodingKeys: String, CodingKey {
        case documents, documentPages, darkDocuments, sheets, darkSheets, presentations, pdfs, darkPdfs, texts
    }

    public init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        documents = try values.decode(Bool.self, forKey: .documents)
        documentPages = try values.decode(Bool.self, forKey: .documentPages)
        darkDocuments = try values.decode(Bool.self, forKey: .darkDocuments)
        sheets = try values.decode(Bool.self, forKey: .sheets)
        darkSheets = try values.decode(Bool.self, forKey: .darkSheets)
        presentations = try values.decode(Bool.self, forKey: .presentations)
        pdfs = try values.decode(Bool.self, forKey: .pdfs)
        darkPdfs = try values.decode(Bool.self, forKey: .darkPdfs)
        texts = try values.decode(Bool.self, forKey: .texts)
    }

    public func encode(to encoder: any Encoder) throws {
        var values = encoder.container(keyedBy: CodingKeys.self)
        try values.encode(documents, forKey: .documents)
        try values.encode(documentPages, forKey: .documentPages)
        try values.encode(darkDocuments, forKey: .darkDocuments)
        try values.encode(sheets, forKey: .sheets)
        try values.encode(darkSheets, forKey: .darkSheets)
        try values.encode(presentations, forKey: .presentations)
        try values.encode(pdfs, forKey: .pdfs)
        try values.encode(darkPdfs, forKey: .darkPdfs)
        try values.encode(texts, forKey: .texts)
    }
}

/// `@AppStorage` keeps the viewers as JSON text.
extension DocumentViewers: RawRepresentable {
    public init?(rawValue: String) {
        guard let viewers = try? JSONDecoder().decode(Self.self, from: Data(rawValue.utf8)) else { return nil }
        self = viewers
    }

    public var rawValue: String {
        (try? JSONEncoder().encode(self)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
    }
}
