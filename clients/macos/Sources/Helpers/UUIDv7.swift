import Foundation

enum UUIDv7 {
    static func generate(now: Date = Date(), random: UUID = UUID()) -> String {
        let timestamp = UInt64(max(0, now.timeIntervalSince1970 * 1000))
        var bytes = [UInt8](repeating: 0, count: 16)
        bytes[0] = UInt8((timestamp >> 40) & 0xFF)
        bytes[1] = UInt8((timestamp >> 32) & 0xFF)
        bytes[2] = UInt8((timestamp >> 24) & 0xFF)
        bytes[3] = UInt8((timestamp >> 16) & 0xFF)
        bytes[4] = UInt8((timestamp >> 8) & 0xFF)
        bytes[5] = UInt8(timestamp & 0xFF)

        withUnsafeBytes(of: random.uuid) { randomBytes in
            for index in 6 ..< 16 {
                bytes[index] = randomBytes[index]
            }
        }
        bytes[6] = (bytes[6] & 0x0F) | 0x70
        bytes[8] = (bytes[8] & 0x3F) | 0x80

        let uuid = uuid_t(
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15]
        )
        return UUID(uuid: uuid).uuidString.lowercased()
    }
}
