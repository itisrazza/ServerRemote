import SwiftUI

/// The app's name and version, where it lives, who made it, and its
/// licences.
struct AboutView: View {
    @Environment(AppModel.self) private var model
    private let about = aboutInfo()

    var body: some View {
        Form {
            Section {
                VStack(spacing: 8) {
                    IconView(.logo, size: 96)
                    Text("ServerRemote").font(.title.bold())
                    Text(about.version).foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity)
                .padding(.vertical, 8)
            }
            Section {
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
