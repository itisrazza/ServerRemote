import SwiftUI

struct DisplaySettings: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        let settings = model.state.settings
        Form {
            Section {
                ChoicePicker(
                    title: "setting-scaling",
                    value: settings.scaling,
                    options: [.fit, .actual],
                    label: { choiceLabel(choice: .scaling(value: $0)) },
                    change: { .scaling(value: $0) }
                )
            } footer: {
                Text(model.localize("setting-scaling-description"))
            }
            Section {
                Toggle(model.localize("setting-keep-aspect-ratio"), isOn: Binding(
                    get: { settings.keepAspectRatio },
                    set: { model.core.changeSetting(change: .keepAspectRatio(on: $0)) }
                ))
                .disabled(settings.scaling == .actual)
            } footer: {
                Text(model.localize("setting-keep-aspect-ratio-description"))
            }
        }
    }
}
