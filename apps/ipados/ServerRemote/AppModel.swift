import CoreGraphics
import Foundation
import Observation

/// The app's state for SwiftUI, read from the shared Rust core whenever it
/// says something changed. Everything the app does goes through `core`.
@MainActor
@Observable
final class AppModel {
    let core: Core
    private(set) var state: AppState
    /// The screen of each connection that has one.
    private(set) var screens: [UInt64: CGImage] = [:]
    /// A request from the core waiting for the user (a file picker).
    var request: ShellRequest?

    private var refreshQueued = false

    init() throws {
        let bridge = ShellBridge()
        core = try Core(configDir: Self.configDirectory()?.path, shell: bridge)
        state = core.state()
        bridge.model = self
    }

    /// Reads the state again, once per run loop however often the core
    /// reports a change.
    func scheduleRefresh() {
        guard !refreshQueued else { return }
        refreshQueued = true
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            refreshQueued = false
            refresh()
        }
    }

    private func refresh() {
        state = core.state()
        let open = Set(state.connections.map(\.id))
        screens = screens.filter { open.contains($0.key) }
        // Copying a screen out of the core costs a few megabytes, so only
        // the one on show is kept current.
        if case let .connection(id) = state.page, let connection = connection(id),
           connection.frames > 0 {
            updateScreen(connection)
        }
    }

    private var shownFrames: [UInt64: UInt64] = [:]

    private func updateScreen(_ connection: ConnectionState) {
        guard shownFrames[connection.id] != connection.frames,
              let screen = core.screen(connection: connection.id),
              let image = CGImage.rgba(screen)
        else { return }
        shownFrames[connection.id] = connection.frames
        screens[connection.id] = image
    }

    func localize(_ id: String, _ args: [String: String] = [:]) -> String {
        args.isEmpty ? core.localize(id: id) : core.localizeArgs(id: id, args: args)
    }

    func connection(_ id: UInt64) -> ConnectionState? {
        state.connections.first { $0.id == id }
    }

    /// Settings and saved sessions live in Application Support.
    private static func configDirectory() -> URL? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
    }
}

/// Receives the core's callbacks, which arrive on its own threads, and
/// hands them to the model on the main thread.
private final class ShellBridge: Shell, @unchecked Sendable {
    weak var model: AppModel?

    func changed() {
        DispatchQueue.main.async { [weak self] in
            self?.model?.scheduleRefresh()
        }
    }

    func perform(request: ShellRequest) {
        DispatchQueue.main.async { [weak self] in
            self?.model?.request = request
        }
    }
}

extension CGImage {
    /// A remote screen as an image, ignoring the alpha byte.
    static func rgba(_ screen: Screen) -> CGImage? {
        guard let provider = CGDataProvider(data: screen.rgba as CFData) else { return nil }
        return CGImage(
            width: Int(screen.width),
            height: Int(screen.height),
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: Int(screen.width) * 4,
            space: CGColorSpace(name: CGColorSpace.sRGB)!,
            bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.noneSkipLast.rawValue),
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
        )
    }
}
