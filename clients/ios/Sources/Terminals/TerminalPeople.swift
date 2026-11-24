import Foundation
import KodosiBridge
import KodosiDesign

extension AppModel {
    func viewers(of session: RuntimeSession) -> [AvatarStack.Person] {
        (session.connectedUsers ?? [])
            .filter { $0 != account.userId }
            .map { AvatarStack.Person(id: $0, name: personName($0)) }
    }

    func personName(_ id: String) -> String {
        if id == account.userId {
            return account.selfName
        }
        if let name = people.name(of: id) {
            return name
        }
        if let member = projects.member(id) {
            return member.personName
        }
        return String(localized: "Guest")
    }

    func sharedNames(of session: RuntimeSession) -> [String] {
        session.sharedWith.map { personName($0) }
    }
}
