import Foundation
import KodosiBridge

/// The live viewers of this device, and what each one shows. A viewer that is off leaves its
/// files to Quick Look.
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

    public init() {}

    public func shows(_ view: CanvasItem.View) -> Bool {
        switch view {
        case .docx: documents
        case .xlsx: sheets
        case .pptx: presentations
        case .pdf: pdfs
        }
    }

    /// The choices that the view of this kind takes.
    func preferences(for view: CanvasItem.View) -> [String: Bool] {
        switch view {
        case .docx: ["pages": documentPages, "dark": darkDocuments]
        case .xlsx: ["dark": darkSheets]
        case .pptx: [:]
        case .pdf: ["dark": darkPdfs]
        }
    }
}

extension DocumentViewers: Codable {
    private enum CodingKeys: String, CodingKey {
        case documents, documentPages, darkDocuments, sheets, darkSheets, presentations, pdfs, darkPdfs
    }

    public init(from decoder: any Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        let chosen = { (key: CodingKeys, usual: Bool) in try values.decodeIfPresent(Bool.self, forKey: key) ?? usual }
        let usual = Self()
        documents = try chosen(.documents, usual.documents)
        documentPages = try chosen(.documentPages, usual.documentPages)
        darkDocuments = try chosen(.darkDocuments, usual.darkDocuments)
        sheets = try chosen(.sheets, usual.sheets)
        darkSheets = try chosen(.darkSheets, usual.darkSheets)
        presentations = try chosen(.presentations, usual.presentations)
        pdfs = try chosen(.pdfs, usual.pdfs)
        darkPdfs = try chosen(.darkPdfs, usual.darkPdfs)
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
