@testable import KodosiDesign
import Testing

@Test func `a table in an agent's answer has its columns, their sides and the bars that its words hold`() {
    let blocks = ProseBlock.blocks(of: #"""
    Prices:
    | SKU | price \| EUR | note |
    |:---|---:|:-:|
    | MUG-01 | 2400 | `a` |
    | BOWL-02 | 3800 |
    After.
    """#)

    guard blocks.count == 3, case let .table(table) = blocks[1] else {
        Issue.record("the answer has words, a table and words: \(blocks)")
        return
    }
    #expect(blocks[0] == .lines("Prices:"))
    #expect(blocks[2] == .lines("After."))
    #expect(table.header == ["SKU", "price | EUR", "note"])
    #expect(table.rows == [["MUG-01", "2400", "`a`"], ["BOWL-02", "3800"]])
    #expect((0 ... 3).map(table.side(of:)) == [.leading, .trailing, .center, .leading])
    #expect(table.columns == 3)
}

@Test func `rows with bars and no row of dashes are a table without names of its columns`() {
    let table = ProseTable(lines: ["| a | b |", "| c | d |"])

    #expect(table.header.isEmpty)
    #expect(table.rows == [["a", "b"], ["c", "d"]])
}
