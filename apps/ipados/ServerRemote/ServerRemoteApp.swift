import SwiftUI

@main
struct ServerRemoteApp: App {
    @State private var model: AppModel?
    @State private var failure: String?

    var body: some Scene {
        WindowGroup {
            Group {
                if let model {
                    RootView()
                        .environment(model)
                } else if let failure {
                    ContentUnavailableView(
                        "ServerRemote couldn't start",
                        systemImage: "exclamationmark.triangle",
                        description: Text(failure)
                    )
                } else {
                    ProgressView()
                }
            }
            .task {
                do {
                    model = try AppModel()
                } catch {
                    failure = String(describing: error)
                }
            }
        }
    }
}
