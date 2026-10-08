import SwiftUI

struct MissionsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var name = ""
    @State private var creating = false
    @State private var showsCreate = false

    var body: some View {
        if !deps.accountReady {
            SignInView().padding(28).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        } else {
            HStack(spacing: 0) {
                VStack(alignment: .leading, spacing: 12) {
                    HStack {
                        Text("Rooms").appTextStyle(.headingItem)
                        Spacer()
                        Button { showsCreate = true } label: { Image(systemName: "plus") }
                            .buttonStyle(.plain).accessibilityLabel(Text("Create room"))
                            .accessibilityIdentifier("missions.create")
                    }.padding(.horizontal, 12).padding(.top, 14)
                    if deps.missionListTruncated {
                        Image(systemName: "ellipsis.circle").foregroundStyle(theme.colors.mutedForeground)
                            .help("Room list limited").accessibilityLabel(Text("Some rooms are not shown")).padding(.horizontal, 12)
                    }
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 4) {
                            ForEach(deps.missions) { mission in
                                Button { deps.openMission(mission) } label: {
                                    Text(mission.name).appTextStyle(.body).frame(maxWidth: .infinity, alignment: .leading)
                                        .padding(12).background(deps.workbench.selectedMissionId == mission.id ? theme.colors.secondary : .clear)
                                }.buttonStyle(.plain)
                                    .accessibilityIdentifier("missions.mission.\(AccessibilityIdentifier.token(mission.id))")
                            }
                            ForEach(deps.invitations) { invitation in
                                VStack(alignment: .leading, spacing: 8) {
                                    Text(invitation.missionName).appTextStyle(.body)
                                    Text("Invited by \(invitation.inviterName)").appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
                                    HStack {
                                        Button("Join") { respond(invitation, accept: true) }
                                        Button("Decline") { respond(invitation, accept: false) }
                                    }
                                }.padding(12)
                            }
                        }
                    }
                }
                .frame(width: 184).background(theme.colors.surfacePanel).seamBorder(.trailing)
                if let detail = deps.missionDetail, detail.mission.id == deps.workbench.selectedMissionId {
                    MissionDetailView(detail: detail, state: deps.roomView(detail.mission.id))
                } else {
                    if deps.workbench.selectedMissionId != nil {
                        RoomSkeleton()
                    } else {
                        VStack(spacing: 20) {
                            Image(systemName: "square.stack.3d.up").font(.system(size: 54, weight: .ultraLight)).foregroundStyle(theme.colors.primary)
                            Button("New room") { showsCreate = true }.buttonStyle(SolidPrimaryButtonStyle())
                        }.frame(maxWidth: .infinity, maxHeight: .infinity)
                    }
                }
            }
            .sheet(isPresented: $showsCreate) {
                VStack(alignment: .leading, spacing: 16) {
                    Text("Create room").appTextStyle(.headingSection)
                    TextField("Name", text: $name).textFieldStyle(KodosiTextFieldStyle())
                    HStack {
                        Spacer()
                        Button("Cancel") { showsCreate = false }.keyboardShortcut(.cancelAction)
                        Button {
                            creating = true
                            Task { @MainActor in
                                defer { creating = false }
                                do {
                                    let id = UUIDv7.generate()
                                    _ = try await deps.commandSink.request("mission.create", [
                                        "requestId": .string(id),
                                        "name": .string(name.trimmingCharacters(in: .whitespacesAndNewlines)),
                                    ])
                                    showsCreate = false; name = ""
                                    deps.openMission(id: id)
                                } catch { deps.errorMessage = error.localizedDescription }
                            }
                        } label: {
                            ZStack {
                                Text("Create").opacity(creating ? 0 : 1); if creating {
                                    ProgressView().controlSize(.small)
                                }
                            }
                        }.disabled(creating || !ProductInput.validName(name))
                            .buttonStyle(SolidPrimaryButtonStyle())
                    }
                }.padding(24).frame(width: 430).background(theme.colors.background)
            }
        }
    }

    private func respond(_ invitation: MissionInvitationEntry, accept: Bool) {
        Task { @MainActor in
            do {
                let operation = accept ? "mission.invitation.accept" : "mission.invitation.reject"
                _ = try await deps.commandSink.request(operation, ["invitationId": .string(invitation.id)])
                if accept {
                    deps.openMission(id: invitation.missionId)
                }

            } catch {
                deps.errorMessage = error.localizedDescription
            }
        }
    }
}
