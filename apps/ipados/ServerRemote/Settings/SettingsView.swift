import SwiftUI

/// The settings sections, each a page of its own. A new section is a case
/// here and a view in this folder.
enum SettingsSection: CaseIterable, Hashable {
    case general
    case display
    case security
    case about

    var title: String {
        switch self {
        case .general: "section-general"
        case .display: "section-display"
        case .security: "section-security"
        case .about: "section-about"
        }
    }

    var symbol: String {
        switch self {
        case .general: "gear"
        case .display: "display"
        case .security: "lock.shield"
        case .about: "info.circle"
        }
    }

    var colour: Color {
        switch self {
        case .general: .gray
        case .display: .brand
        case .security: .green
        case .about: .gray
        }
    }

    @MainActor @ViewBuilder
    var page: some View {
        switch self {
        case .general: GeneralSettings()
        case .display: DisplaySettings()
        case .security: SecuritySettings()
        case .about: AboutView()
        }
    }
}

/// The list of sections, as in the Settings app.
struct SettingsView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        List {
            Section {
                ForEach([SettingsSection.general, .display, .security], id: \.self, content: row)
            }
            Section {
                row(.about)
            }
        }
        .navigationTitle(model.localize("tab-settings"))
        .navigationDestination(for: SettingsSection.self) { section in
            section.page
                .navigationTitle(model.localize(section.title))
        }
    }

    private func row(_ section: SettingsSection) -> some View {
        NavigationLink(value: section) {
            Label {
                Text(model.localize(section.title))
            } icon: {
                Image(systemName: section.symbol)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 28, height: 28)
                    .background(section.colour, in: RoundedRectangle(cornerRadius: 7, style: .continuous))
            }
        }
    }
}

/// A setting with a few named choices, applied as soon as one is picked.
struct ChoicePicker<Value: Hashable>: View {
    @Environment(AppModel.self) private var model
    let title: String
    let value: Value
    let options: [Value]
    let label: (Value) -> String
    let change: (Value) -> SettingChange

    var body: some View {
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
