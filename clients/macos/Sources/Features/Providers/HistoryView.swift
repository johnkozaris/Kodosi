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
                ProgressView()
            }
        }
        .frame(width: 900, height: 620)
        .background(theme.colors.background)
        .buttonStyle(SolidSecondaryButtonStyle())
        .textFieldStyle(KodosiTextFieldStyle())
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
            HStack {
                Text("History").appTextStyle(.headingSection)
                Spacer()
                SessionIconButton(title: "Close history", symbol: "xmark", identifier: "panel.resume.done") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }.padding(16)
            HStack(spacing: 12) {
                KodosiPicker("Provider", selection: $browser.provider, values: Provider.allCases, label: { $0.name })
                    .frame(width: 230).accessibilityIdentifier("panel.resume.provider")
                Text(browser.directory.isEmpty ? String(localized: "All projects") : browser.directory)
                    .appTextStyle(.caption).lineLimit(1).truncationMode(.middle)
                Spacer()
                if !browser.directory.isEmpty {
                    Button("All projects") { browser.directory = ""; browser.load() }
                }
                Button("Choose folder…") {
                    if let directory = pickWorkingDirectory(initialDirectory: browser.directory) {
                        browser.directory = directory
                        browser.load()
                    }
                }.accessibilityIdentifier("panel.resume.folder")
            }.padding(.horizontal, 16).padding(.bottom, 12)
            HStack(spacing: 0) {
                conversationList(browser)
                preview(browser)
            }
            if let message = browser.message {
                Text(message).appTextStyle(.caption).foregroundStyle(theme.colors.destructive).padding(12)
            }
            HStack {
                Spacer()
                Button(resuming ? String(localized: "Starting…") : String(localized: "Resume")) { resume(browser) }
                    .buttonStyle(SolidPrimaryButtonStyle()).disabled(browser.selected == nil || resuming)
                    .accessibilityIdentifier("panel.resume.start")
            }.padding(16)
        }
        .onChange(of: browser.provider) { _, _ in browser.load() }
    }

    private func conversationList(_ browser: ConversationBrowser) -> some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 4) {
                if browser.loading {
                    ProgressView().frame(maxWidth: .infinity).padding()
                }
                ForEach(browser.conversations) { conversation in
                    Button { browser.select(conversation) } label: {
                        VStack(alignment: .leading, spacing: 5) {
                            Text(conversation.title ?? URL(fileURLWithPath: conversation.workingDirectory).lastPathComponent).appTextStyle(.body).lineLimit(2)
                            Text(URL(fileURLWithPath: conversation.workingDirectory).lastPathComponent)
                                .appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
                            if let updated = conversation.updatedAt {
                                Text(historyDate(updated)).appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
                            }
                        }
                        .padding(10).frame(maxWidth: .infinity, alignment: .leading)
                        .background(browser.selected?.id == conversation.id ? theme.colors.secondary : .clear)
                        .contentShape(Rectangle())
                    }.buttonStyle(.plain)
                        .accessibilityIdentifier("panel.resume.conversation.\(AccessibilityIdentifier.token(conversation.id))")
                }
                if browser.nextCursor != nil {
                    Button("Load more") { browser.load(more: true) }.disabled(browser.loading).padding(10)
                        .accessibilityIdentifier("panel.resume.more")
                }
            }
        }.frame(width: 260).background(theme.colors.surfacePanel)
    }

    private func preview(_ browser: ConversationBrowser) -> some View {
        VStack(spacing: 0) {
            if browser.selected == nil {
                Text(browser.conversations.isEmpty && !browser.loading
                    ? String(localized: "No saved conversations in this folder.")
                    : String(localized: "Select a conversation to preview it."))
                    .foregroundStyle(theme.colors.mutedForeground).appTextStyle(.body)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                HStack(spacing: 10) {
                    Button("Earlier") { browser.read(older: true) }.disabled(browser.nextBeforeByte == nil || browser.reading)
                    Button("Later") { browser.newer() }.disabled(!browser.canReadNewer || browser.reading)
                    Spacer()
                    Text(ByteCountFormatter.string(fromByteCount: Int64(clamping: browser.sourceFileBytes), countStyle: .file))
                        .appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
                    if browser.canReadNewer {
                        Button("Latest") { browser.latest() }.disabled(browser.reading)
                    }
                }.padding(12).seamBorder(.bottom)
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 18) {
                            Color.clear.frame(height: 1).id("pageTop")
                            ForEach(Array(browser.entries.enumerated()), id: \.offset) { _, entry in
                                HistoryMessageView(entry: entry)
                            }
                            if browser.reading {
                                ProgressView().frame(maxWidth: .infinity)
                            }
                            if browser.entries.isEmpty, !browser.reading {
                                Text("No messages in this page. Use Earlier to continue through the file.")
                                    .appTextStyle(.body).foregroundStyle(theme.colors.mutedForeground)
                            }
                        }.padding(20).id("\(browser.selected?.id ?? ""):\(browser.pageNumber)")
                    }
                    .onChange(of: browser.pageNumber) { _, _ in proxy.scrollTo("pageTop", anchor: .top) }
                    .onChange(of: browser.selected?.id) { _, _ in proxy.scrollTo("pageTop", anchor: .top) }
                }
            }
        }
    }

    private func historyDate(_ value: String) -> String {
        let parser = ISO8601DateFormatter()
        parser.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let date = parser.date(from: value) ?? ISO8601DateFormatter().date(from: value)
        return date?.formatted(date: .abbreviated, time: .shortened) ?? String(value.prefix(10))
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
