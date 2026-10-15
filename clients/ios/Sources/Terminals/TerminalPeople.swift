import Foundation
import KodosiBridge
import KodosiDesign

extension AppModel {
    func viewers(of session: RuntimeSession) -> [AvatarStack.Person] {
        (session.connectedUsers ?? [])
            .filter { $0 != account.userId }
            .map { AvatarStack.Person(id: $0, name: personName($0, in: session)) }
    }

    func personName(_ id: String, in session: RuntimeSession) -> String {
        if id == account.userId {
            return account.selfName
        }
        if let name = people.name(of: id) {
            return name
        }
        if let room = session.missionId,
           let member = rooms.details[room]?.members.first(where: { $0.userId == id })
        {
            return member.personName
        }
        return String(localized: "Guest")
    }

    func sharedNames(of session: RuntimeSession) -> [String] {
        session.sharedWith.map { personName($0, in: session) }
    }
}
