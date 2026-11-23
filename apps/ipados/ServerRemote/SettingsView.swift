import SwiftUI

/// Settings, as a grouped form, with About at the end.
struct SettingsView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Form {
            general
            display
            security
            AboutSection()
        }
        .navigationTitle(model.localize("tab-settings"))
    }

    private var settings: SettingsState {
        model.state.settings
    }

    private var general: some View {
        Section(model.localize("section-general")) {
            choicePicker(
                "setting-appearance",
                value: settings.appearance,
                options: [.system, .light, .dark],
                label: { choiceLabel(choice: .appearance(value: $0)) },
                change: { .appearance(value: $0) }
            )
            choicePicker(
                "setting-language",
                value: settings.language,
                options: [.system, .english, .romanian],
                label: { choiceLabel(choice: .language(value: $0)) },
                change: { .language(value: $0) }
            )
        }
    }

    private var display: some View {
        Section {
            choicePicker(
                "setting-scaling",
                value: settings.scaling,
                options: [.fit, .actual],
                label: { choiceLabel(choice: .scaling(value: $0)) },
                change: { .scaling(value: $0) }
            )
            Toggle(model.localize("setting-keep-aspect-ratio"), isOn: Binding(
                get: { settings.keepAspectRatio },
                set: { model.core.changeSetting(change: .keepAspectRatio(on: $0)) }
            ))
            .disabled(settings.scaling == .actual)
        } header: {
            Text(model.localize("section-display"))
        } footer: {
            Text(model.localize("setting-keep-aspect-ratio-description"))
        }
    }

    @ViewBuilder
    private var security: some View {
        Section {
            choicePicker(
                "setting-certificate-checking",
                value: settings.certificateChecking,
                options: [.strict, .ask, .off],
                label: { choiceLabel(choice: .certificateChecking(value: $0)) },
                change: { .certificateChecking(value: $0) }
            )
        } header: {
            Text(model.localize("section-security"))
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

    private func choicePicker<Value: Hashable>(
        _ title: String,
        value: Value,
        options: [Value],
        label: @escaping (Value) -> String,
        change: @escaping (Value) -> SettingChange
    ) -> some View {
        Picker(model.localize(title), selection: Binding(
            get: { value },
            set: { model.core.changeSetting(change: change($0)) }
        )) {
            ForEach(options, id: \.self) { option in
                Text(label(option)).tag(option)
            }
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

/// The app's name and version, where it lives, who made it, and its
/// licences.
private struct AboutSection: View {
    @Environment(AppModel.self) private var model
    private let about = aboutInfo()

    var body: some View {
        Section(model.localize("section-about")) {
            HStack(spacing: 16) {
                IconView(.logo, size: 64)
                VStack(alignment: .leading) {
                    Text("ServerRemote").font(.title2.bold())
                    Text(about.version).foregroundStyle(.secondary)
                }
            }
            .padding(.vertical, 4)
            link(model.localize("about-website"), about.websiteUrl, systemImage: "globe")
            link("GitHub", about.projectUrl, systemImage: "chevron.left.forwardslash.chevron.right")
            LabeledContent(model.localize("about-made-by").trimmingCharacters(in: [":", " "])) {
                HStack(spacing: -6) {
                    ForEach(about.contributors, id: \.login) { contributor in
                        if let avatar = UIImage(data: contributor.avatar) {
                            Image(uiImage: avatar)
                                .resizable()
                                .frame(width: 28, height: 28)
                                .clipShape(Circle())
                                .overlay(Circle().stroke(Color(.systemBackground), lineWidth: 2))
                                .accessibilityLabel(contributor.login)
                        }
                    }
                }
            }
        }
        Section {
            link(model.localize("about-mit-license"), about.mitLicenseUrl, systemImage: "doc.text")
            link(model.localize("about-apache-license"), about.apacheLicenseUrl, systemImage: "doc.text")
            NavigationLink(model.localize("about-third-party-licences")) {
                ThirdPartyLicences(carbonUrl: about.carbonUrl)
            }
        } header: {
            Text(model.localize("about-licence"))
        } footer: {
            Text(model.localize("about-licence-description"))
        }
    }

    @ViewBuilder
    private func link(_ title: String, _ url: String, systemImage: String) -> some View {
        if let url = URL(string: url) {
            Link(destination: url) {
                Label(title, systemImage: systemImage)
            }
        }
    }
}

private struct ThirdPartyLicences: View {
    @Environment(AppModel.self) private var model
    let carbonUrl: String

    var body: some View {
        Form {
            Section {
                Text(model.localize("licences-icons"))
            }
            if let url = URL(string: carbonUrl) {
                Section {
                    Link("Carbon", destination: url)
                }
            }
        }
        .navigationTitle(model.localize("licences-title"))
    }
}
