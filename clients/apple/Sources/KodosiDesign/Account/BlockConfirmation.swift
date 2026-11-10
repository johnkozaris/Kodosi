import SwiftUI

public extension View {
    /// Asks before the user blocks a person.
    func blockConfirmation(isPresented: Binding<Bool>, name: String, block: @escaping () -> Void) -> some View {
        confirmationDialog(Text("Block \(name)?", bundle: .module), isPresented: isPresented, titleVisibility: .visible) {
            Button(role: .destructive, action: block) { Text("Block", bundle: .module) }
        } message: {
            Text("""
            They cannot send you friend requests. If you are friends, you stop sharing terminals with each other, \
            and each of you leaves the projects that the other owns.
            """, bundle: .module)
        }
    }
}
