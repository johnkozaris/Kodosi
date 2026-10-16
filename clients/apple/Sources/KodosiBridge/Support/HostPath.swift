public enum HostPath {
    public static func folderName(_ path: String) -> String {
        let separators: Set<Character> = isWindows(path) ? ["/", "\\"] : ["/"]
        return path.split(whereSeparator: separators.contains).last.map(String.init) ?? path
    }

    private static func isWindows(_ path: String) -> Bool {
        if path.hasPrefix(#"\\"#) {
            return true
        }
        let start = Array(path.prefix(3))
        return start.count == 3 && start[0].isASCII && start[0].isLetter && start[1] == ":" && "\\/".contains(start[2])
    }
}
