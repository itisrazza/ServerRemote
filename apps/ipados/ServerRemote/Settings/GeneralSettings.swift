import SwiftUI

struct GeneralSettings: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let settings = model.state.settings
        Form {
            Section {
                ChoicePicker(
                    title: "setting-appearance",
                    value: settings.appearance,
                    options: [.system, .light, .dark],
                    label: { choiceLabel(choice: .appearance(value: $0)) },
                    change: { .appearance(value: $0) }
                )
            } footer: {
                Text(model.localize("setting-appearance-description"))
            }
            Section {
                ChoicePicker(
                    title: "setting-language",
                    value: settings.language,
                    options: [.system, .english, .romanian],
                    label: { choiceLabel(choice: .language(value: $0)) },
                    change: { .language(value: $0) }
                )
            }
        }
    }
}
