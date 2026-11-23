import OSLog
import SwiftUI

/// Icons that belong to the content rather than the chrome: each protocol's,
/// a connector's own, and the app's logo, rendered by the core. Template
/// icons take the foreground colour; pictures are drawn as they are.
struct IconView: View {
    enum Source: Hashable {
        case connectionProtocol(ConnectionProtocol)
        /// A connector's icon, by connection and the id in its records.
        case connector(connection: UInt64, id: UInt64)
        case logo
    }

    @Environment(AppModel.self) private var model
    @Environment(\.displayScale) private var displayScale
    let source: Source
    var size: CGFloat = 20

    init(_ source: Source, size: CGFloat = 20) {
        self.source = source
        self.size = size
    }

    var body: some View {
        Group {
            if let image = IconCache.shared.image(source, pixels: pixels, core: model.core) {
                image.resizable().interpolation(.high)
            } else {
                Color.clear
            }
        }
        .frame(width: size, height: size)
    }

    private var pixels: UInt32 {
        UInt32((size * displayScale).rounded())
    }
}

/// Rendered icons, kept for the life of the app: there are only a few dozen.
@MainActor
final class IconCache {
    static let shared = IconCache()

    private struct Key: Hashable {
        let source: IconView.Source
        let pixels: UInt32
    }

    private var images: [Key: Image] = [:]
    private let log = Logger(subsystem: "io.razza.ServerRemote", category: "icons")

    /// The icon `pixels` across, for drawing at `pixels / displayScale`.
    func image(_ source: IconView.Source, pixels: UInt32, core: Core) -> Image? {
        let key = Key(source: source, pixels: pixels)
        if let image = images[key] { return image }
        guard let icon = render(source, pixels: pixels, core: core),
              let image = UIImage(data: icon.png)
        else { return nil }
        let rendered = Image(uiImage: image).renderingMode(icon.template ? .template : .original)
        images[key] = rendered
        return rendered
    }

    private func render(_ source: IconView.Source, pixels: UInt32, core: Core) -> IconImage? {
        do {
            switch source {
            case let .connectionProtocol(kind):
                return try protocolIcon(protocol: kind, pixels: pixels)
            case let .connector(connection, id):
                return try core.connectionIcon(connection: connection, icon: id, pixels: pixels)
            case .logo:
                return try logo(pixels: pixels)
            }
        } catch {
            log.error("couldn't render icon \(String(describing: source)): \(error)")
            return nil
        }
    }
}

extension Color {
    /// The desktop app's accent, so both feel like the same app.
    static let brand = Color(red: 0x4C / 255, green: 0x8D / 255, blue: 0xFF / 255)
}

extension Indicator {
    var color: Color {
        switch self {
        case .ok: .green
        case .warning: .yellow
        case .error: .red
        case .inactive: .secondary
        }
    }
}

extension ConnectionPhase {
    var color: Color {
        switch self {
        case .connecting: .yellow
        case .connected: .green
        case .disconnected: .red
        }
    }
}

/// A connector's icon: the matching SF Symbol for the built-in ones, so it
/// sits evenly with the app's own toolbar symbols, otherwise its own image.
struct ConnectorIcon: View {
    let connection: UInt64
    let icon: IconRef
    var size: CGFloat = 20

    var body: some View {
        if let symbol = icon.builtin.flatMap({ Self.symbols[$0] }) {
            Image(systemName: symbol)
        } else {
            IconView(.connector(connection: connection, id: icon.id), size: size)
        }
    }

    /// The built-in icons (`lib/icons/assets`) connectors use, as SF Symbols.
    private static let symbols: [String: String] = [
        "carbon_add": "plus",
        "carbon_add-large": "plus",
        "carbon_arrow-down": "arrow.down",
        "carbon_arrow-left": "arrow.left",
        "carbon_arrow-right": "arrow.right",
        "carbon_arrow-up": "arrow.up",
        "carbon_arrows-horizontal": "arrow.left.and.right",
        "carbon_camera": "camera",
        "carbon_checkmark": "checkmark",
        "carbon_chevron-down": "chevron.down",
        "carbon_chevron-left": "chevron.left",
        "carbon_chevron-right": "chevron.right",
        "carbon_chevron-up": "chevron.up",
        "carbon_close": "xmark",
        "carbon_close-large": "xmark",
        "carbon_copy-to-clipboard": "doc.on.doc",
        "carbon_cursor-1": "cursorarrow",
        "carbon_diamond-outline": "diamond",
        "carbon_down-to-bottom": "arrow.down.to.line",
        "carbon_download": "arrow.down.circle",
        "carbon_folder": "folder",
        "carbon_information": "info.circle",
        "carbon_keyboard": "keyboard",
        "carbon_launch": "arrow.up.forward.square",
        "carbon_locked": "lock",
        "carbon_power": "power",
        "carbon_progress-bar-round": "circle.dotted",
        "carbon_restart": "arrow.clockwise",
        "carbon_save": "square.and.arrow.down",
        "carbon_screen": "display",
        "carbon_settings": "gear",
        "carbon_up-to-top": "arrow.up.to.line",
        "carbon_upload": "arrow.up.circle",
        "carbon_virtual-desktop": "display.2",
        "carbon_vmdk-disk": "opticaldiscdrive",
        "key_backspace": "delete.left",
        "key_delete": "delete.right",
        "key_enter": "return",
    ]
}
