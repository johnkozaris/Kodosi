import SwiftUI

struct HistoryView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    @State private var browser: ConversationBrowser?
    @State private var resuming = false

    var body: some View {
        Group {
            if let browser {
                content(browser)
            } else {
                ProgressCaption(text: String(localized: "Loading"))
            }
        }
        .frame(width: 940, height: 640)
        .background(theme.colors.raised.mix(with: theme.colors.surface, by: 0.4))
        .buttonStyle(.kodosi(.secondary))
        .textFieldStyle(WellTextFieldStyle())
        .onAppear {
            browser = ConversationBrowser(
                commands: deps.commandSink,
                directory: deps.selectedLocalDirectory ?? ""
            )
            browser?.load()
        }
        .onDisappear { browser?.cancel() }
        .onChange(of: deps.accountEpoch) { _, _ in dismiss() }
    }

    private func content(_ browser: ConversationBrowser) -> some View {
        @Bindable var browser = browser
        return VStack(spacing: 0) {
            HStack(spacing: 12) {
                Text("Resume a conversation").appTextStyle(.title).foregroundStyle(theme.colors.ink)
                Spacer()
                SegmentedPill(selection: $browser.provider, options: Provider.allCases.map {
                    SegmentOption(value: $0, title: $0.name)
                }, compact: true, identifier: "panel.resume.provider")
                Button {
                    if let directory = pickWorkingDirectory(initialDirectory: browser.directory) {
                        browser.directory = directory
                        browser.load()
                    }
                } label: {
                    Label(browser.directory.isEmpty ? String(localized: "All projects") : URL(fileURLWithPath: browser.directory).lastPathComponent,
                          systemImage: "folder")
                }
                .buttonStyle(.kodosi(.secondary, size: .small)).help("Choose a project folder")
                .accessibilityIdentifier("panel.resume.folder")
                if !browser.directory.isEmpty {
                    IconButton(title: "All projects", symbol: "xmark.circle", identifier: "panel.resume.allProjects", size: 24) {
                        browser.directory = ""; browser.load()
                    }
                }
                IconButton(title: "Close", symbol: "xmark", identifier: "panel.resume.done") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(.leading, 22).padding(.trailing, 14).frame(height: 60)
            HStack(spacing: 0) {
                if browser.loading || !browser.conversations.isEmpty {
                    conversationList(browser)
                }
                preview(browser)
            }
            if let message = browser.message {
                ErrorNote(message: message).padding(.horizontal, 16).padding(.top, 8)
            }
            HStack(spacing: 8) {
                if browser.selected != nil {
                    IconButton(title: "Earlier", symbol: "chevron.up", identifier: "panel.resume.earlier", size: 26) { browser.read(older: true) }
                        .disabled(browser.nextBeforeByte == nil || browser.reading)
                    IconButton(title: "Later", symbol: "chevron.down", identifier: "panel.resume.later", size: 26) { browser.newer() }
                        .disabled(!browser.canReadNewer || browser.reading)
                    if browser.canReadNewer {
                        Button("Latest") { browser.latest() }.buttonStyle(.kodosi(.ghost, size: .small)).disabled(browser.reading)
                    }
                    Text(ByteCountFormatter.string(fromByteCount: Int64(clamping: browser.sourceFileBytes), countStyle: .file))
                        .appTextStyle(.caption).foregroundStyle(theme.colors.inkFaint)
                }
                Spacer()
                Button { resume(browser) } label: {
                    Label(resuming ? String(localized: "Starting…") : String(localized: "Resume in a new terminal"), systemImage: "play.fill")
                        .shimmer(resuming)
                }
                .buttonStyle(.kodosi(.primary, size: .large)).disabled(browser.selected == nil || resuming)
                .accessibilityIdentifier("panel.resume.start")
            }
            .padding(.horizontal, 16).frame(height: 64)
        }
        .onChange(of: browser.provider) { _, _ in browser.load() }
    }

    private func conversationList(_ browser: ConversationBrowser) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 2) {
                if browser.loading {
                    ForEach(0 ..< 5, id: \.self) { _ in
                        VStack(alignment: .leading, spacing: 7) { SkeletonBlock(height: 10); SkeletonBlock(width: 110, height: 8) }.padding(12)
                    }
                }
                ForEach(browser.conversations) { conversation in
                    let selected = browser.selected?.id == conversation.id
                    Button { browser.select(conversation) } label: {
                        VStack(alignment: .leading, spacing: 5) {
                            Text(conversation.title ?? URL(fileURLWithPath: conversation.workingDirectory).lastPathComponent)
                                .appTextStyle(.body).fontWeight(.medium).foregroundStyle(theme.colors.ink).lineLimit(2)
                                .multilineTextAlignment(.leading)
                            HStack(spacing: 6) {
                                Image(systemName: "folder").font(.system(size: 9))
                                Text(URL(fileURLWithPath: conversation.workingDirectory).lastPathComponent).lineLimit(1)
                                if let updated = conversation.updatedAt {
                                    Text(verbatim: "·")
                                    Text(historyDate(updated)).lineLimit(1)
                                }
                            }
                            .appTextStyle(.caption).fontWeight(.regular).foregroundStyle(theme.colors.inkFaint)
                        }
                        .padding(.horizontal, 12).padding(.vertical, 10).frame(maxWidth: .infinity, alignment: .leading)
                        .background {
                            if selected {
                                RaisedBackground(shape: RoundedRectangle(cornerRadius: Radius.lg, style: .continuous),
                                                 fill: theme.colors.raised, elevation: .resting)
                            }
                        }
                        .hoverRow(radius: Radius.lg, selected: selected)
                    }
                    .buttonStyle(.plain)
                    .accessibilityIdentifier("panel.resume.conversation.\(AccessibilityIdentifier.token(conversation.id))")
                }
                if browser.nextCursor != nil {
                    Button("Show more") { browser.load(more: true) }.buttonStyle(.kodosi(.ghost, size: .small))
                        .disabled(browser.loading).padding(10)
                        .accessibilityIdentifier("panel.resume.more")
                }
            }
            .padding(8)
        }
        .frame(width: 290)
        .well(Radius.xl)
        .padding(.leading, 14)
    }

    private func preview(_ browser: ConversationBrowser) -> some View {
        Group {
            if browser.selected == nil {
                EmptyState(
                    title: browser.conversations.isEmpty && !browser.loading ? "Nothing saved here" : "Pick a conversation",
                    message: browser.conversations.isEmpty && !browser.loading
                        ? "Choose another project, or switch the agent." : "You see it here before you resume it."
                ) {
                    AgentMark(kind: browser.provider.agent, size: 52, activity: .asleep)
                } actions: { EmptyView() }
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 16) {
                            Color.clear.frame(height: 1).id("pageTop")
                            ForEach(Array(browser.entries.enumerated()), id: \.offset) { _, entry in
                                HistoryMessageView(entry: entry, agent: browser.provider.agent)
                            }
                            if browser.reading {
                                ProgressCaption(text: String(localized: "Reading")).frame(maxWidth: .infinity)
                            }
                            if browser.entries.isEmpty, !browser.reading {
                                Text("No messages on this page. Go earlier.")
                                    .appTextStyle(.footnote).foregroundStyle(theme.colors.inkMuted)
                            }
                        }
                        .padding(20).id("\(browser.selected?.id ?? ""):\(browser.pageNumber)")
                    }
                    .onChange(of: browser.pageNumber) { _, _ in proxy.scrollTo("pageTop", anchor: .top) }
                    .onChange(of: browser.selected?.id) { _, _ in proxy.scrollTo("pageTop", anchor: .top) }
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func historyDate(_ value: String) -> String {
        guard let date = RoomDates.parse(value) else { return String(value.prefix(10)) }
        return date.formatted(.relative(presentation: .named))
    }

    private func resume(_ browser: ConversationBrowser) {
        guard let selected = browser.selected else { return }
        resuming = true
        Task { @MainActor in
            defer { resuming = false }
            do {
                try await deps.createSession(name: selected.sessionName,
                                             directory: selected.workingDirectory, resume: selected.identity)
                dismiss()
            } catch { browser.message = error.localizedDescription }
        }
    }
}
