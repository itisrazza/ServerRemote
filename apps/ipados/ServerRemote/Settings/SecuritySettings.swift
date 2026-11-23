import SwiftUI

struct SecuritySettings: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let settings = model.state.settings
        Form {
            Section {
                ChoicePicker(
                    title: "setting-certificate-checking",
                    value: settings.certificateChecking,
                    options: [.strict, .ask, .off],
                    label: { choiceLabel(choice: .certificateChecking(value: $0)) },
                    change: { .certificateChecking(value: $0) }
                )
            } footer: {
                Text(certificateCheckingDescription(value: settings.certificateChecking))
            }
            CertificateList(
                title: "trusted-certificates",
                description: "trusted-certificates-description",
                action: "certificate-forget",
                certificates: settings.trusted,
                change: { .forgetTrusted(index: $0) }
            )
            CertificateList(
                title: "blocked-certificates",
                description: "blocked-certificates-description",
                action: "certificate-unblock",
                certificates: settings.blocked,
                change: { .unblockBlocked(index: $0) }
            )
        }
    }
}

/// Saved certificates, each with a button to take it off the list.
private struct CertificateList: View {
    @Environment(AppModel.self) private var model
    let title: String
    let description: String
    let action: String
    let certificates: [Certificate]
    let change: (UInt32) -> SettingChange

    var body: some View {
        Section {
            if certificates.isEmpty {
                Text(model.localize("certificates-none")).foregroundStyle(.secondary)
            }
            ForEach(Array(certificates.enumerated()), id: \.offset) { index, certificate in
                HStack {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("\(certificate.host):\(certificate.port)")
                        Text(certificate.fingerprint)
                            .font(.caption.monospaced())
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                    }
                    Spacer()
                    Button(model.localize(action)) {
                        model.core.changeSetting(change: change(UInt32(index)))
                    }
                    .buttonStyle(.bordered)
                }
            }
        } header: {
            Text(model.localize(title))
        } footer: {
            Text(model.localize(description))
        }
    }
}
