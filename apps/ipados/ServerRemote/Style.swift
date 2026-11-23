import CoreText
import SwiftUI

/// Sizes from the desktop app (`apps/desktop/src/ui/style.rs`), scaled up
/// for touch.
enum Metrics {
    static let scale: CGFloat = 1.25
    static let controlHeight: CGFloat = 28 * scale
    static let iconSize: CGFloat = 16 * scale
    static let spacing: CGFloat = 4 * scale
    static let radius: CGFloat = 6 * scale
    static let panelRadius: CGFloat = 8 * scale
    static let textSize: CGFloat = 13 * scale
    static let titleSize: CGFloat = 16 * scale
}

/// The desktop app's palette, following the system's light or dark mode.
extension Color {
    /// The window.
    static let base = grey(dark: 0x12, light: 0xF0)
    static let weaker = grey(dark: 0x1C, light: 0xF8)
    /// Panels, menus and dialogs.
    static let weak = grey(dark: 0x21, light: 0xFF)
    /// Hover and quiet fills.
    static let neutral = grey(dark: 0x2C, light: 0xE6)
    /// Borders, dividers, the active tab's fill.
    static let strong = grey(dark: 0x41, light: 0xD2)
    /// Dialog borders and menu separators.
    static let stronger = grey(dark: 0x4D, light: 0xC4)
    /// The active tab's border, secondary text.
    static let strongest = grey(dark: 0x68, light: 0x76)
    static let text = rgb(dark: 0xFFFFFF, light: 0x1A1A1A)
    static let accent = rgb(dark: 0x4C8DFF, light: 0x2F6FEB)
    static let success = rgb(dark: 0x00C950, light: 0x00A843)
    static let warning = rgb(dark: 0xFFD230, light: 0xF0B400)
    static let danger = rgb(dark: 0xC42B1C, light: 0xC42B1C)

    private static func grey(dark: UInt8, light: UInt8) -> Color {
        let value = { (grey: UInt8) -> UInt32 in
            UInt32(grey) << 16 | UInt32(grey) << 8 | UInt32(grey)
        }
        return rgb(dark: value(dark), light: value(light))
    }

    private static func rgb(dark: UInt32, light: UInt32) -> Color {
        Color(UIColor { traits in
            let hex = traits.userInterfaceStyle == .dark ? dark : light
            return UIColor(
                red: CGFloat((hex >> 16) & 0xFF) / 255,
                green: CGFloat((hex >> 8) & 0xFF) / 255,
                blue: CGFloat(hex & 0xFF) / 255,
                alpha: 1
            )
        })
    }
}

/// Nunito Sans, registered from the core, which carries the desktop app's
/// font files.
enum Fonts {
    private(set) static var medium = "NunitoSans-Medium"
    private(set) static var bold = "NunitoSans-Bold"

    static func register() {
        let names = fonts().compactMap { data -> String? in
            guard let provider = CGDataProvider(data: data as CFData),
                  let font = CGFont(provider)
            else { return nil }
            CTFontManagerRegisterGraphicsFont(font, nil)
            return font.postScriptName as String?
        }
        if let name = names.first { medium = name }
        if names.count > 1 { bold = names[1] }
    }
}

extension Font {
    /// The desktop app's text: Nunito Sans Medium.
    static var ui: Font { .custom(Fonts.medium, fixedSize: Metrics.textSize) }
    static var uiBold: Font { .custom(Fonts.bold, fixedSize: Metrics.textSize) }
    static var uiTitle: Font { .custom(Fonts.bold, fixedSize: Metrics.titleSize) }
}

// MARK: - Buttons

/// The title bar's and toolbar's outlined buttons; `active` is filled, as
/// the tab on show is.
struct BorderedStyle: ButtonStyle {
    var active = false

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(active ? .uiBold : .ui)
            .foregroundStyle(Color.text)
            .frame(minHeight: Metrics.controlHeight)
            .background(
                fill(pressed: configuration.isPressed),
                in: RoundedRectangle(cornerRadius: Metrics.radius)
            )
            .overlay(
                RoundedRectangle(cornerRadius: Metrics.radius)
                    .strokeBorder(active ? Color.strongest : Color.strong)
            )
            .contentShape(RoundedRectangle(cornerRadius: Metrics.radius))
    }

    private func fill(pressed: Bool) -> Color {
        active || pressed ? .strong : .clear
    }
}

/// A button set into a panel: the window's colour, outlined, e.g. the
/// manager's cards and toolbar.
struct InsetStyle: ButtonStyle {
    var radius = Metrics.radius

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.ui)
            .foregroundStyle(Color.text)
            .background(
                configuration.isPressed ? Color.weaker : Color.base,
                in: RoundedRectangle(cornerRadius: radius)
            )
            .overlay(
                RoundedRectangle(cornerRadius: radius)
                    .strokeBorder(configuration.isPressed ? Color.strongest : Color.strong)
            )
            .contentShape(RoundedRectangle(cornerRadius: radius))
    }
}

/// A dialog's or form's buttons.
struct DialogButtonStyle: ButtonStyle {
    enum Kind { case primary, secondary, destructive }

    var kind: Kind
    @Environment(\.isEnabled) private var enabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.ui)
            .padding(.horizontal, 12 * Metrics.scale)
            .frame(minHeight: Metrics.controlHeight)
            .foregroundStyle(kind == .secondary ? Color.text : Color.base)
            .background(fill(pressed: configuration.isPressed), in: RoundedRectangle(cornerRadius: Metrics.radius))
            .overlay {
                if kind == .secondary {
                    RoundedRectangle(cornerRadius: Metrics.radius).strokeBorder(Color.strong)
                }
            }
            .opacity(enabled ? 1 : 0.5)
            .contentShape(RoundedRectangle(cornerRadius: Metrics.radius))
    }

    private func fill(pressed: Bool) -> Color {
        switch kind {
        case .primary: pressed ? .accent.opacity(0.8) : .accent
        case .destructive: pressed ? .danger.opacity(0.8) : .danger
        case .secondary: pressed ? .strong : .clear
        }
    }
}

/// A sidebar entry; `active` shares the panel's fill beside it.
struct SidebarItemStyle: ButtonStyle {
    var active: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.ui)
            .foregroundStyle(Color.text)
            .frame(maxWidth: .infinity, minHeight: Metrics.controlHeight * 1.15, alignment: .leading)
            .padding(.horizontal, 8 * Metrics.scale)
            .background(
                active ? Color.weak : configuration.isPressed ? Color.weaker : .clear,
                in: RoundedRectangle(cornerRadius: Metrics.radius)
            )
            .contentShape(RoundedRectangle(cornerRadius: Metrics.radius))
    }
}

// MARK: - Containers

extension View {
    /// The rounded panel a page's content sits in.
    func panel() -> some View {
        background(Color.weak, in: RoundedRectangle(cornerRadius: Metrics.panelRadius))
    }
}

/// A one-line text field outlined like the desktop's, lit in the accent
/// colour while focused.
struct OutlinedField: View {
    let placeholder: String
    @Binding var text: String
    var secure = false
    @FocusState private var focused: Bool

    var body: some View {
        Group {
            if secure {
                SecureField("", text: $text, prompt: prompt)
            } else {
                TextField("", text: $text, prompt: prompt)
            }
        }
        .font(.ui)
        .foregroundStyle(Color.text)
        .textInputAutocapitalization(.never)
        .autocorrectionDisabled()
        .focused($focused)
        .padding(.horizontal, 8 * Metrics.scale)
        .frame(height: Metrics.controlHeight)
        .background(Color.base.opacity(0.0001))
        .overlay(
            RoundedRectangle(cornerRadius: Metrics.radius)
                .strokeBorder(focused ? Color.accent : Color.strong)
        )
    }

    private var prompt: Text {
        Text(placeholder).foregroundStyle(Color.strongest)
    }
}

struct VerticalDivider: View {
    var height: CGFloat = 20 * Metrics.scale

    var body: some View {
        Rectangle().fill(Color.strong).frame(width: 1, height: height)
    }
}

/// A status bar's coloured dot.
struct StatusDot: View {
    let color: Color

    var body: some View {
        Circle().fill(color).frame(width: 8 * Metrics.scale, height: 8 * Metrics.scale)
    }
}
