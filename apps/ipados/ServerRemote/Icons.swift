import OSLog
import SwiftUI

/// The desktop app's icons, rendered by the core and tinted to the text
/// colour unless they're pictures.
struct IconView: View {
    enum Source: Hashable {
        case app(AppIcon)
        case connectionProtocol(ConnectionProtocol)
        /// A connector's icon, by connection and the id in its records.
        case connector(connection: UInt64, id: UInt64)
    }

    @Environment(AppModel.self) private var model
    @Environment(\.displayScale) private var displayScale
    let source: Source
    var size: CGFloat = Metrics.iconSize

    init(_ icon: AppIcon, size: CGFloat = Metrics.iconSize) {
        source = .app(icon)
        self.size = size
    }

    init(_ source: Source, size: CGFloat = Metrics.iconSize) {
        self.source = source
        self.size = size
    }

    var body: some View {
        Group {
            if let rendered = IconCache.shared.image(source, pixels: pixels, core: model.core) {
                Image(uiImage: rendered.image)
                    .renderingMode(rendered.template ? .template : .original)
                    .resizable()
                    .interpolation(.high)
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

    struct Rendered {
        let image: UIImage
        let template: Bool
    }

    private struct Key: Hashable {
        let source: IconView.Source
        let pixels: UInt32
    }

    private var images: [Key: Rendered] = [:]

    func image(_ source: IconView.Source, pixels: UInt32, core: Core) -> Rendered? {
        let key = Key(source: source, pixels: pixels)
        if let image = images[key] { return image }
        guard let icon = render(source, pixels: pixels, core: core),
              let image = UIImage(data: icon.png)
        else { return nil }
        let rendered = Rendered(image: image, template: icon.template)
        images[key] = rendered
        return rendered
    }

    private let log = Logger(subsystem: "io.razza.ServerRemote", category: "icons")

    private func render(_ source: IconView.Source, pixels: UInt32, core: Core) -> IconImage? {
        do {
            switch source {
            case let .app(icon):
                return try appIcon(icon: icon, pixels: pixels)
            case let .connectionProtocol(kind):
                return try protocolIcon(protocol: kind, pixels: pixels)
            case let .connector(connection, id):
                return try core.connectionIcon(connection: connection, icon: id, pixels: pixels)
            }
        } catch {
            log.error("couldn't render icon \(String(describing: source)): \(error)")
            return nil
        }
    }
}
