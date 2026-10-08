enum TerminalShellEscape {
    static func escape(_ string: String) -> String {
        var result = "'"
        result.reserveCapacity(string.utf8.count + 2)
        for character in string {
            if character == "'" {
                result.append("'\\''")
            } else {
                result.append(character)
            }
        }
        result.append("'")
        return result
    }
}
