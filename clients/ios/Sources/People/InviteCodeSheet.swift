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

    private var card: some View {
        ScanCodeCard {
            if let rows = people.ownInviteCode {
                ScanCode(rows: rows)
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
        .frame(maxWidth: 360)
    }
}
