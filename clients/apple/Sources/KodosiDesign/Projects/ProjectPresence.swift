import KodosiBridge
import SwiftUI

public extension View {
    func showsProject(_ projectId: String, in projects: ProjectStore) -> some View {
        modifier(ProjectShown(projectId: projectId, projects: projects))
    }

    func showsProjectPart(_ part: ProjectPart, of state: ProjectViewState) -> some View {
        onAppear { state.show(part) }
            .onDisappear { state.hide(part) }
    }
}

private struct ProjectShown: ViewModifier {
    let projectId: String
    let projects: ProjectStore

    func body(content: Content) -> some View {
        content
            .task(id: projectId) { await projects.open(projectId) }
            .onAppear { projects.visibleProjectId = projectId }
            .onChange(of: projectId) { _, projectId in projects.visibleProjectId = projectId }
            .onDisappear {
                if projects.visibleProjectId == projectId {
                    projects.visibleProjectId = nil
                }
            }
    }
}
