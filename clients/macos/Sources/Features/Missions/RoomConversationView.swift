import AppKit
import SwiftUI

struct RoomConversationView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let roomId: String
    @Bindable var state: RoomViewState
    @State private var copied = false
    private var messages: [RoomMessage] {
        deps.rooms[roomId]?.messages ?? []
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Label("Conversation", systemImage: "bubble.left.and.bubble.right").font(.system(size: 13, weight: .semibold))
                Spacer()
                Button(action: copyInstructions) { Image(systemName: copied ? "checkmark" : "sparkles") }
                    .contentTransition(.symbolEffect(.replace)).buttonStyle(.plain)
                    .help("Copy agent instructions").accessibilityLabel(Text("Copy agent instructions"))
                    .accessibilityIdentifier("room.agent.instructions")
            }.padding(16).frame(height: 52).seamBorder(.bottom)
            if deps.rooms[roomId] == nil {
                if let failure = state.failure, !state.loading {
                    RoomRecovery(message: failure) { Task { await deps.readRoom(roomId) } }
                } else {
                    RoomSkeleton()
                }
            } else {
                transcript
                if let failure = state.failure {
                    RoomActionError(message: failure) { state.failure = nil }
                }
                composer
            }
        }.background(theme.colors.background)
    }

    private var transcript: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 22) {
                    if deps.rooms[roomId]?.hasOlder == true, let first = messages.first {
                        HStack {
                            Spacer()
                            Button {
                                Task { await deps.readRoom(roomId, before: first.sequence); proxy.scrollTo(first.id, anchor: .top) }
                            } label: { Image(systemName: "arrow.up") }
                                .help("Earlier messages").accessibilityLabel(Text("Earlier messages"))
                                .accessibilityIdentifier("room.messages.earlier").disabled(state.loading)
                            Spacer()
                        }
                    }
                    ForEach(messages) { message in messageRow(message).id(message.id) }
                    Color.clear.frame(height: 1).id("latest")
                }.scrollTargetLayout().padding(18)
            }
            .defaultScrollAnchor(.bottom)
            .scrollPosition(id: $state.readingMessage, anchor: .bottom)
            .onScrollGeometryChange(for: Bool.self) { geometry in
                geometry.contentOffset.y + geometry.containerSize.height >= geometry.contentSize.height - 36
            } action: { _, latest in
                state.followsLatest = latest
                if latest {
                    state.unread = 0
                }
            }
            .overlay(alignment: .center) {
                if messages.isEmpty {
                    Image(systemName: "bubble.left.and.bubble.right").font(.system(size: 42, weight: .ultraLight))
                        .foregroundStyle(theme.colors.mutedForeground.opacity(0.5))
                }
            }
            .overlay(alignment: .bottom) {
                if !state.followsLatest {
                    Button {
                        withAnimation(reduceMotion ? nil : .smooth(duration: 0.2)) { proxy.scrollTo("latest", anchor: .bottom) }
                    } label: {
                        Label(state.unread > 0 ? String(localized: "\(state.unread) new") : String(localized: "Latest"), systemImage: "arrow.down")
                    }.buttonStyle(SolidSecondaryButtonStyle()).clipShape(Capsule()).padding(10)
                        .accessibilityIdentifier("room.messages.latest")
                }
            }
            .onChange(of: messages.last?.id) { _, _ in
                if state.followsLatest {
                    withAnimation(reduceMotion ? nil : .smooth(duration: 0.2)) { proxy.scrollTo("latest", anchor: .bottom) }
                }
            }
        }
    }

    private func messageRow(_ message: RoomMessage) -> some View {
        HStack(alignment: .top, spacing: 10) {
            RoomAvatar(name: message.authorName, size: 26)
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 6) {
                    Text(message.authorName).font(.system(size: 12, weight: .semibold))
                    if let agent = message.agent {
                        Label(agent, systemImage: "sparkles").font(.system(size: 10)).foregroundStyle(theme.colors.primary)
                    }
                    Spacer(minLength: 0)
                    if let date = date(message.createdAt) {
                        Text(date, style: .time).font(.system(size: 10)).foregroundStyle(theme.colors.mutedForeground)
                    }
                }
                Text(.init(message.text)).appTextStyle(.body).textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }.accessibilityIdentifier("room.message.\(AccessibilityIdentifier.token(message.id))")
            .transition(.opacity.combined(with: .offset(y: reduceMotion ? 0 : 6)))
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: 8) {
            TextField("Message", text: $state.message, axis: .vertical)
                .lineLimit(1 ... 6).textFieldStyle(.plain).padding(.vertical, 10).padding(.leading, 12)
                .accessibilityLabel(Text("Message the room")).accessibilityIdentifier("room.message.input")
                .onSubmit {
                    if !state.busy {
                        send()
                    }
                }
            Button(action: send) {
                ZStack {
                    if state.busy {
                        ProgressView().controlSize(.small)
                    } else {
                        Image(systemName: "arrow.up").font(.system(size: 14, weight: .semibold))
                    }
                }.frame(width: 30, height: 30).background(theme.colors.primary, in: Circle()).foregroundStyle(theme.colors.primaryForeground)
            }.buttonStyle(.plain).disabled(state.busy || state.message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                .accessibilityLabel(Text("Send message")).accessibilityIdentifier("room.message.send").padding(6)
        }.background(theme.colors.surfacePanel, in: RoundedRectangle(cornerRadius: 16))
            .overlay { RoundedRectangle(cornerRadius: 16).stroke(theme.colors.border.opacity(0.5), lineWidth: 0.5) }
            .padding(12)
    }

    private func date(_ value: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter.date(from: value) ?? ISO8601DateFormatter().date(from: value)
    }

    private func send() {
        Task { await deps.postRoomMessage(roomId) }
    }

    private func copyInstructions() {
        let path = (Bundle.main.executablePath ?? "kodosi").replacingOccurrences(of: "'", with: "'\"'\"'")
        NSPasteboard.general.clearContents()
        let instructions = """
        Read `'\(path)' room skill`, then use \
        `'\(path)' --json room --room \(roomId) context`.
        Use this executable for the skill's kodosi commands. You can read and post updates, \
        create and pick up tasks, and share results for this work.
        """
        NSPasteboard.general.setString(instructions, forType: .string)
        copied = true
        Task { try? await Task.sleep(for: .seconds(2)); copied = false }
    }
}
