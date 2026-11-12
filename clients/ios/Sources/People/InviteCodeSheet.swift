import KodosiBridge
import KodosiDesign
import SwiftUI

struct InviteCodeSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss

    private var people: PeopleStore {
        model.people
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 22) {
                    card
                        .arrive()
                    Text("Your friend scans this code with their camera to add you.")
                        .appTextStyle(.callout)
                        .foregroundStyle(theme.colors.inkMuted)
                        .multilineTextAlignment(.center)
                        .fixedSize(horizontal: false, vertical: true)
                        .arrive(delay: 0.04)
                    if let invite = people.ownInvite {
                        ShareLink(item: invite, preview: SharePreview(Text("My Kodosi invite"))) {
                            Label("Share my invite", systemImage: "square.and.arrow.up")
                                .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.kodosi(.secondary, size: .large))
                        .accessibilityIdentifier("people.invite.share")
                        .transition(.opacity)
                    }
                }
                .frame(maxWidth: 400)
                .frame(maxWidth: .infinity)
                .padding(.horizontal, 24)
                .padding(.vertical, 16)
            }
            .scrollBounceBehavior(.basedOnSize)
            .background(theme.colors.surface)
            .navigationTitle("Your invite")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    IconButton(title: "Close", symbol: "xmark", identifier: "people.invite.close", size: 32) { dismiss() }
                }
                .sharedBackgroundVisibility(.hidden)
            }
        }
        .presentationDetents([.large])
        .kodosiSheet()
        .animation(theme.motion.soft, value: people.ownInviteCode)
        .task {
            if people.ownInvite == nil {
                people.requestInvite()
            }
        }
    }

    /// The card stays light in both appearances, as a printed code does, so that each camera
    /// reads its dark modules.
    private var card: some View {
        ZStack {
            Color.clear
            if let rows = people.ownInviteCode {
                InviteCode(rows: rows)
                    .accessibilityElement()
                    .accessibilityLabel(Text("QR code of your invite"))
                    .accessibilityAddTraits(.isImage)
                    .accessibilityIdentifier("people.invite.code")
                    .transition(.opacity)
            } else if people.fetchingInvite {
                Spinner(size: 18)
            } else {
                Button("Try again") { people.requestInvite() }
                    .buttonStyle(.kodosi(.secondary))
                    .accessibilityIdentifier("people.invite.retry")
            }
        }
        .aspectRatio(1, contentMode: .fit)
        .frame(maxWidth: 360)
        .raised(Radius.xl, fill: AppTheme.Colors.light.raised)
        .environment(\.theme, AppTheme.light.reducingMotion(theme.motion.reduced))
        .environment(\.colorScheme, .light)
    }
}

private struct InviteCode: View {
    @Environment(\.theme) private var theme
    @Environment(\.displayScale) private var displayScale
    let rows: [String]

    private static let quietZone = 4

    var body: some View {
        Canvas { context, size in
            let count = CGFloat(rows.count + 2 * Self.quietZone)
            let module = (min(size.width, size.height) * displayScale / count).rounded(.down) / displayScale
            let left = ((size.width - module * CGFloat(rows.count)) / 2 * displayScale).rounded(.down) / displayScale
            let top = ((size.height - module * CGFloat(rows.count)) / 2 * displayScale).rounded(.down) / displayScale
            var dark = Path()
            for (y, row) in rows.enumerated() {
                for (x, cell) in row.utf8.enumerated() where cell == UInt8(ascii: "1") {
                    dark.addRect(CGRect(x: left + CGFloat(x) * module, y: top + CGFloat(y) * module, width: module, height: module))
                }
            }
            context.fill(dark, with: .color(theme.colors.ink))
        }
    }
}
