import SwiftUI
import UIKit

/// Asks the key capture view to take or give up keyboard focus. Holding
/// focus brings up the on-screen keyboard when no hardware one is attached.
final class KeyboardFocus {
    fileprivate weak var view: KeyCaptureView?

    func focus() {
        guard let view, !view.isFirstResponder else { return }
        view.becomeFirstResponder()
    }

    func toggle() {
        guard let view else { return }
        if view.isFirstResponder {
            view.resignFirstResponder()
        } else {
            view.becomeFirstResponder()
        }
    }
}

/// Sends the keyboard to a connection: hardware keys as HID usages (so
/// modifiers and function keys work), on-screen typing as text.
struct KeyCapture: UIViewRepresentable {
    @Environment(AppModel.self) private var model
    let connection: UInt64
    let focus: KeyboardFocus

    func makeUIView(context: Context) -> KeyCaptureView {
        let view = KeyCaptureView()
        focus.view = view
        return view
    }

    func updateUIView(_ view: KeyCaptureView, context: Context) {
        let core = model.core
        let connection = connection
        view.onKey = { usage, pressed in
            core.hidKey(connection: connection, usage: usage, pressed: pressed)
        }
        view.onText = { text in
            core.typeText(connection: connection, text: text)
        }
        focus.view = view
    }
}

final class KeyCaptureView: UIView, UIKeyInput {
    /// Sends a hardware key; returns whether it was taken.
    var onKey: ((UInt32, Bool) -> Bool)?
    var onText: ((String) -> Void)?

    // The on-screen keyboard types exactly what's pressed.
    var autocorrectionType: UITextAutocorrectionType = .no
    var autocapitalizationType: UITextAutocapitalizationType = .none
    var spellCheckingType: UITextSpellCheckingType = .no
    var smartQuotesType: UITextSmartQuotesType = .no
    var smartDashesType: UITextSmartDashesType = .no
    var smartInsertDeleteType: UITextSmartInsertDeleteType = .no

    override var canBecomeFirstResponder: Bool { true }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window != nil {
            becomeFirstResponder()
        }
    }

    var hasText: Bool { true }

    func insertText(_ text: String) {
        onText?(text)
    }

    func deleteBackward() {
        onText?("\u{8}")
    }

    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = send(presses, pressed: true)
        if !rest.isEmpty { super.pressesBegan(rest, with: event) }
    }

    override func pressesEnded(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = send(presses, pressed: false)
        if !rest.isEmpty { super.pressesEnded(rest, with: event) }
    }

    override func pressesCancelled(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        let rest = send(presses, pressed: false)
        if !rest.isEmpty { super.pressesCancelled(rest, with: event) }
    }

    /// Sends the presses it can, returning the rest for the system.
    private func send(_ presses: Set<UIPress>, pressed: Bool) -> Set<UIPress> {
        presses.filter { press in
            guard let key = press.key, let onKey else { return true }
            return !onKey(UInt32(key.keyCode.rawValue), pressed)
        }
    }
}
