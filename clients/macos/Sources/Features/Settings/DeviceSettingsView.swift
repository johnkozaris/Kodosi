import SwiftUI

struct DeviceSettingsView: View {
    @Environment(AppDependencies.self) private var deps
    @Environment(\.theme) private var theme
    @State private var code = ""
    @State private var revoke: MyDeviceEntry?
    @State private var signingOut = false
    @State private var deletingAccount = false

    var body: some View {
        VStack(alignment: .leading, spacing: 22) {
            HStack {
                Text("Account & Devices").appTextStyle(.headingSection)
                Spacer()
                if deps.userId != nil {
                    Button("Delete Account…", role: .destructive) { deletingAccount = true }
                        .accessibilityIdentifier("settings.account.delete")
                    Button("Sign Out…") { signingOut = true }
                        .accessibilityIdentifier("settings.account.signOut")
                }
            }
            if deps.userId == nil {
                SignInView()
            } else {
                if !deps.localDeviceEnrolled {
                    Text("Trust this Mac").appTextStyle(.headingItem)
                    DeviceTrustView(trust: deps.deviceTrust)
                }
                Text("Your devices").appTextStyle(.headingItem)
                ForEach(deps.devices) { device in
                    HStack {
                        Image(systemName: "laptopcomputer").font(.system(size: 26, weight: .light)).foregroundStyle(theme.colors.primary)
                        VStack(alignment: .leading, spacing: 4) {
                            Text(device.label).appTextStyle(.body)
                            if device.deviceId == deps.selfDeviceId {
                                Text("This Mac").appTextStyle(.caption).foregroundStyle(theme.colors.mutedForeground)
                            }
                        }
                        Spacer()
                        if deps.localDeviceEnrolled, device.deviceId != deps.selfDeviceId {
                            Button("Remove…", role: .destructive) { revoke = device }
                                .accessibilityIdentifier("settings.devices.remove.\(device.deviceId)")
                        }
                    }
                }
                if deps.localDeviceEnrolled {
                    Divider()
                    Text("Approve another device").appTextStyle(.headingItem)
                    ForEach(deps.deviceRequests) { request in
                        Text("\(request.deviceLabel) wants to use your account").appTextStyle(.body)
                    }
                    HStack {
                        TextField("Code shown on the other device", text: $code).textFieldStyle(KodosiTextFieldStyle())
                            .accessibilityIdentifier("settings.devices.code")
                        Button("Approve") {
                            deps.perform("devices.link.approve", ["code": .string(code.trimmingCharacters(in: .whitespacesAndNewlines))])
                            code = ""
                        }
                        .buttonStyle(SolidPrimaryButtonStyle())
                        .disabled(code.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                        .accessibilityIdentifier("settings.devices.approve")
                    }
                }
            }
        }
        .confirmationDialog("Remove device access?", isPresented: Binding(get: { revoke != nil }, set: {
            if !$0 {
                revoke = nil
            }
        }), titleVisibility: .visible) {
            if let device = revoke {
                Button("Remove \(device.label)", role: .destructive) { deps.perform("devices.revoke", ["deviceId": .string(device.deviceId)]); revoke = nil }
            }
        } message: { Text("The device will disconnect and must be approved again to reconnect. Commands it already ran are not undone.") }
        .confirmationDialog("Sign out?", isPresented: $signingOut, titleVisibility: .visible) {
            Button("Sign Out", role: .destructive) { deps.perform("auth.logout") }
        } message: { Text("Remote connections will close. Local terminals remain on this Mac.") }
        .confirmationDialog("Delete your Kodosi account?", isPresented: $deletingAccount, titleVisibility: .visible) {
            Button("Delete Account", role: .destructive) { deps.perform("auth.deleteAccount") }
        } message: {
            Text(
                "This removes your account, devices, friends, missions and shared terminals from the Kodosi server. Local terminals remain on this Mac. "
                    + "If Kodosi refuses, sign out, sign in again, and then delete the account."
            )
        }
        .onAppear {
            if deps.userId != nil {
                deps.perform("devices.refresh")
            }
        }
    }
}
