import KodosiBridge
import KodosiDesign
import SwiftUI

struct CommandPalette: View {
    enum Icon {
        case symbol(String, Color)
        case agent(AgentKind, AgentMark.Activity)
        case room(String)
        case person(name: String, key: String)
    }

    struct Item: Identifiable {
        let id: String
        let group: String
        let title: String
        var detail: String?
        let icon: Icon
        let run: @MainActor () -> Void
    }

    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var query = ""
    @State private var index = 0
    @FocusState private var focused: Bool
    @Namespace private var selection

    private var items: [Item] {
        let needle = query.trimmingCharacters(in: .whitespaces).lowercased()
        let all = terminalItems + roomItems + actionItems + peopleItems
        guard !needle.isEmpty else { return all }
        return all.filter { $0.title.lowercased().contains(needle) || ($0.detail?.lowercased().contains(needle) ?? false) }
    }

    var body: some View {
        let results = items
        ZStack(alignment: .top) {
            Color.black.opacity(theme.isDark ? 0.45 : 0.18).ignoresSafeArea()
                .contentShape(Rectangle()).onTapGesture(perform: close)
            VStack(spacing: 0) {
                HStack(spacing: 10) {
                    Image(systemName: "magnifyingglass").font(.system(size: 13, weight: .semibold)).foregroundStyle(theme.colors.inkMuted)
                    TextField("Go to a terminal, a room or a person", text: $query)
                        .textFieldStyle(.plain).appTextStyle(.body).focused($focused)
                        .onSubmit { run(results) }
                        .onKeyPress(.downArrow) { move(1, in: results); return .handled }
                        .onKeyPress(.upArrow) { move(-1, in: results); return .handled }
                        .onKeyPress(.escape) { close(); return .handled }
                        .accessibilityIdentifier("palette.query")
                    Keycap(text: "esc")
                }
                .padding(.horizontal, 12).frame(height: 38)
                .well(Radius.md)
                .padding(10)
                if results.isEmpty {
                    Text("Nothing matches.").appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                        .frame(maxWidth: .infinity).padding(.top, 14).padding(.bottom, 24)
                } else {
                    list(results)
                }
            }
            .frame(width: 560)
            .raised(Radius.sheet, fill: theme.colors.surface, elevation: .floating)
            .padding(.top, 96)
            .transition(.scale(scale: 0.97, anchor: .top).combined(with: .opacity))
        }
        .onAppear { focused = true }
        .onChange(of: query) { _, _ in index = 0 }
        .onExitCommand(perform: close)
        .accessibilityAddTraits(.isModal)
        .accessibilityIdentifier("palette")
    }

    private func list(_ results: [Item]) -> some View {
        ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 1) {
                    ForEach(Array(results.enumerated()), id: \.element.id) { position, item in
                        if position == 0 || results[position - 1].group != item.group {
                            Text(item.group).appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                                .padding(.horizontal, 12).padding(.top, position == 0 ? 0 : 12).padding(.bottom, 4)
                        }
                        Button { index = position; run(results) } label: { row(item, highlighted: position == index) }
                            .buttonStyle(.plain).id(item.id)
                            .accessibilityIdentifier("palette.item.\(AccessibilityIdentifier.token(item.id))")
                    }
                }
                .padding(.horizontal, 10).padding(.bottom, 10)
            }
            .scrollIndicators(.never)
            .frame(maxHeight: 380)
            .fixedSize(horizontal: false, vertical: true)
            .onChange(of: index) { _, value in
                if results.indices.contains(value) {
                    proxy.scrollTo(results[value].id)
                }
            }
        }
    }

    private func row(_ item: Item, highlighted: Bool) -> some View {
        HStack(spacing: 10) {
            Group {
                switch item.icon {
                case let .symbol(name, tint): IconTile(symbol: name, tint: tint, size: 24)
                case let .agent(kind, activity): AgentMark(kind: kind, size: 24, activity: activity)
                case let .room(key): RoomSigil(key: key, size: 24)
                case let .person(name, key): PersonAvatar(name: name, key: key, size: 24)
                }
            }
            Text(item.title).appTextStyle(.body).fontWeight(.medium).foregroundStyle(theme.colors.ink).lineLimit(1)
            if let detail = item.detail {
                Text(detail).appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted).lineLimit(1)
            }
            Spacer(minLength: 8)
            if highlighted {
                Keycap(symbol: "return").transition(.opacity.combined(with: .offset(x: -4)))
            }
        }
        .padding(.horizontal, 8).frame(height: 36)
        .background {
            if highlighted {
                SelectionPill(namespace: selection)
            }
        }
        .contentShape(Rectangle())
        .animation(theme.motion.snappy, value: highlighted)
    }

    private func move(_ offset: Int, in results: [Item]) {
        guard !results.isEmpty else { return }
        index = (index + offset + results.count) % results.count
    }

    private func run(_ results: [Item]) {
        guard results.indices.contains(index) else { return }
        let item = results[index]
        close()
        item.run()
    }

    private func close() {
        deps.workbench.showsPalette = false
    }

    private var terminalItems: [Item] {
        deps.sessions.map { session in
            Item(
                id: "terminal.\(session.id)", group: String(localized: "Terminals"), title: session.name,
                detail: [session.folderName, session.kind == .remote ? session.hostLabel : nil].compactMap(\.self).joined(separator: " · "),
                icon: .agent(session.agent, deps.activity(of: session))
            ) { deps.activateSession(session.id) }
        }
    }

    private var roomItems: [Item] {
        deps.missions.map { mission in
            Item(id: "room.\(mission.id)", group: String(localized: "Rooms"), title: mission.name,
                 icon: .room(mission.id))
            {
                deps.workbench.section = .missions; deps.openMission(mission)
            }
        }
    }

    private var peopleItems: [Item] {
        deps.friends.map { friend in
            let name = friend.personName
            return Item(id: "person.\(friend.userId)", group: String(localized: "People"), title: name, detail: "@\(friend.handle)",
                        icon: .person(name: name, key: friend.userId))
            {
                deps.workbench.section = .people
            }
        }
    }

    private var actionItems: [Item] {
        let group = String(localized: "Actions")
        var actions = [
            Item(id: "action.terminal", group: group, title: String(localized: "New terminal"), icon: .symbol("plus", TileTint.orange)) {
                deps.newSession()
            },
            Item(id: "action.folder", group: group, title: String(localized: "New terminal in a folder…"), icon: .symbol("folder.fill", TileTint.teal)) {
                if let directory = pickWorkingDirectory(initialDirectory: deps.selectedLocalDirectory) {
                    deps.newSession(directory: directory)
                }
            },
            Item(
                id: "action.resume", group: group, title: String(localized: "Resume a conversation"),
                icon: .symbol("clock.arrow.circlepath", TileTint.amber)
            ) {
                deps.workbench.showsHistory = true
            },
        ]
        if deps.accountReady {
            actions.append(Item(id: "action.room", group: group, title: String(localized: "New room"), icon: .symbol("square.grid.2x2.fill", TileTint.purple)) {
                deps.workbench.showsNewRoom = true
            })
        }
        actions.append(Item(id: "action.people", group: group, title: String(localized: "People"), icon: .symbol("person.2.fill", TileTint.blue)) {
            deps.workbench.section = .people
        })
        actions.append(Item(id: "action.settings", group: group, title: String(localized: "Settings"), icon: .symbol("gearshape.fill", TileTint.graphite)) {
            deps.workbench.section = .settings
        })
        return actions
    }
}

private struct Keycap: View {
    @Environment(\.theme) private var theme
    var text: String?
    var symbol: String?

    var body: some View {
        Group {
            if let symbol {
                Image(systemName: symbol).font(.system(size: 9, weight: .bold))
            } else if let text {
                Text(verbatim: text).appTextStyle(.caption2)
            }
        }
        .foregroundStyle(theme.colors.inkMuted)
        .padding(.horizontal, 6).frame(minWidth: 20, minHeight: 18)
        .background {
            BezelBackground(shape: RoundedRectangle(cornerRadius: Radius.xs, style: .continuous), fill: theme.colors.lifted, bezel: .quiet)
        }
    }
}
