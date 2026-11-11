import SwiftUI

public struct ProjectEarlierKey: View {
    let loading: Bool
    let identifier: String
    let action: () -> Void

    public init(loading: Bool, identifier: String, action: @escaping () -> Void) {
        self.loading = loading
        self.identifier = identifier
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            Label {
                loading ? Text("Loading…", bundle: .module) : Text("Earlier messages", bundle: .module)
            } icon: {
                Image(systemName: "arrow.up")
            }
        }
        .buttonStyle(.kodosi(.ghost, size: .small))
        .frame(maxWidth: .infinity)
        .disabled(loading)
        .accessibilityIdentifier(identifier)
    }
}

public struct ProjectTrimmedNote: View {
    @Environment(\.theme) private var theme

    public init() {}

    public var body: some View {
        Text("Kodosi removed the oldest messages to make space for new ones.", bundle: .module)
            .appTextStyle(.caption)
            .foregroundStyle(theme.colors.inkFaint)
            .multilineTextAlignment(.center)
            .frame(maxWidth: .infinity)
    }
}

public struct ProjectLatestKey: View {
    let unread: Int
    let identifier: String
    let action: () -> Void

    public init(unread: Int, identifier: String, action: @escaping () -> Void) {
        self.unread = unread
        self.identifier = identifier
        self.action = action
    }

    public var body: some View {
        Button(action: action) {
            Label {
                unread > 0 ? Text("\(unread) new", bundle: .module) : Text("Latest", bundle: .module)
            } icon: {
                Image(systemName: "arrow.down")
            }
            .contentTransition(.numericText())
        }
        .buttonStyle(.kodosi(unread > 0 ? .primary : .secondary, size: .small))
        .accessibilityIdentifier(identifier)
    }
}
