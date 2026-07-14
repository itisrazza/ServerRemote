//! A tap-driven on-screen keyboard, for tablets and keyboards that lack some
//! standard PC keys. Ordinary keys resolve to an X11 keysym through the same
//! mapping tables `connection.rs` uses for real hardware input; modifier keys
//! (Shift/Ctrl/Alt/AltGr/Super) have no physical "hold" gesture available
//! from a tap, so they're tracked as a local sticky latch instead.

use iced::keyboard::key::Named;
use iced::widget::{Column, Row, Space, button, container, text};
use iced::{Element, Length};

use super::connection::{KeyboardSize, character_keysym, named_keysym};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierKey {
    Shift,
    Control,
    Alt,
    AltGraph,
    Super,
}

impl ModifierKey {
    /// Same X11 keysym constants `named_keysym` uses for these keys.
    fn keysym(self) -> u32 {
        match self {
            ModifierKey::Shift => 0xFFE1,
            ModifierKey::Control => 0xFFE3,
            ModifierKey::Alt => 0xFFE9,
            ModifierKey::AltGraph => 0xFFEA,
            ModifierKey::Super => 0xFFEB,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum LatchState {
    #[default]
    Off,
    Latched,
    Locked,
}

/// The virtual keyboard's own state: which modifiers are currently latched
/// or locked. Not shared with real hardware input handling.
#[derive(Default)]
pub struct State {
    shift: LatchState,
    control: LatchState,
    alt: LatchState,
    alt_graph: LatchState,
    super_key: LatchState,
}

fn latch(state: &State, modifier: ModifierKey) -> LatchState {
    match modifier {
        ModifierKey::Shift => state.shift,
        ModifierKey::Control => state.control,
        ModifierKey::Alt => state.alt,
        ModifierKey::AltGraph => state.alt_graph,
        ModifierKey::Super => state.super_key,
    }
}

fn latch_mut(state: &mut State, modifier: ModifierKey) -> &mut LatchState {
    match modifier {
        ModifierKey::Shift => &mut state.shift,
        ModifierKey::Control => &mut state.control,
        ModifierKey::Alt => &mut state.alt,
        ModifierKey::AltGraph => &mut state.alt_graph,
        ModifierKey::Super => &mut state.super_key,
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    /// A modifier key was tapped; advances its latch state.
    Modifier(ModifierKey),
    /// An ordinary key was tapped, identified by the X11 keysym it sends.
    Tap(u32),
}

/// Releases any `Latched` (one-shot) modifier, resetting it to `Off`. Called
/// once an ordinary key has been sent, so a one-shot combo like Ctrl+C only
/// holds Ctrl down for that single key.
fn release_latched(state: &mut State) -> Vec<(u32, bool)> {
    let mut events = Vec::new();
    for (latch, modifier) in [
        (&mut state.shift, ModifierKey::Shift),
        (&mut state.control, ModifierKey::Control),
        (&mut state.alt, ModifierKey::Alt),
        (&mut state.alt_graph, ModifierKey::AltGraph),
        (&mut state.super_key, ModifierKey::Super),
    ] {
        if *latch == LatchState::Latched {
            events.push((modifier.keysym(), false));
            *latch = LatchState::Off;
        }
    }
    events
}

/// Returns the ordered `(keysym, pressed)` pairs to send for this tap.
pub fn update(state: &mut State, message: Message) -> Vec<(u32, bool)> {
    match message {
        Message::Modifier(modifier) => {
            let latch = latch_mut(state, modifier);
            let (next, events) = match *latch {
                LatchState::Off => (LatchState::Latched, vec![(modifier.keysym(), true)]),
                LatchState::Latched => (LatchState::Locked, Vec::new()),
                LatchState::Locked => (LatchState::Off, vec![(modifier.keysym(), false)]),
            };
            *latch = next;
            events
        }
        Message::Tap(keysym) => {
            let mut events = vec![(keysym, true), (keysym, false)];
            events.extend(release_latched(state));
            events
        }
    }
}

#[derive(Clone, Copy)]
enum Key {
    /// An ordinary character key; `KeyDef::label` is the character itself.
    Char,
    Named(Named),
    /// A raw X11 keysym, for numpad `KP_*` keys with no `Named` variant.
    Keysym(u32),
    Modifier(ModifierKey),
    Spacer,
}

#[derive(Clone, Copy)]
struct KeyDef {
    key: Key,
    label: &'static str,
    /// Width in "units" — 1.0 is one standard letter key.
    width: f32,
}

const fn c(label: &'static str, width: f32) -> KeyDef {
    KeyDef {
        key: Key::Char,
        label,
        width,
    }
}

const fn n(label: &'static str, named: Named, width: f32) -> KeyDef {
    KeyDef {
        key: Key::Named(named),
        label,
        width,
    }
}

const fn ks(label: &'static str, keysym: u32, width: f32) -> KeyDef {
    KeyDef {
        key: Key::Keysym(keysym),
        label,
        width,
    }
}

const fn m(label: &'static str, modifier: ModifierKey, width: f32) -> KeyDef {
    KeyDef {
        key: Key::Modifier(modifier),
        label,
        width,
    }
}

const fn sp(width: f32) -> KeyDef {
    KeyDef {
        key: Key::Spacer,
        label: "",
        width,
    }
}

// X11 numpad keysyms — no `iced::keyboard::key::Named` variant exists for
// these, so they're sent as raw keysyms via `Key::Keysym`.
const KP_DIVIDE: u32 = 0xFFAF;
const KP_MULTIPLY: u32 = 0xFFAA;
const KP_SUBTRACT: u32 = 0xFFAD;
const KP_ADD: u32 = 0xFFAB;
const KP_DECIMAL: u32 = 0xFFAE;
const KP_ENTER: u32 = 0xFF8D;
const KP_0: u32 = 0xFFB0;
const KP_1: u32 = 0xFFB1;
const KP_2: u32 = 0xFFB2;
const KP_3: u32 = 0xFFB3;
const KP_4: u32 = 0xFFB4;
const KP_5: u32 = 0xFFB5;
const KP_6: u32 = 0xFFB6;
const KP_7: u32 = 0xFFB7;
const KP_8: u32 = 0xFFB8;
const KP_9: u32 = 0xFFB9;

// The "small" layout is a flat supplementary bar (not a full keyboard
// shape), so its rows stay simple and don't need to line up with anything.
static MODIFIER_ROW: [KeyDef; 8] = [
    n("Esc", Named::Escape, 1.0),
    n("Tab", Named::Tab, 1.0),
    m("Ctrl", ModifierKey::Control, 1.5),
    m("Alt", ModifierKey::Alt, 1.5),
    m("Shift", ModifierKey::Shift, 1.5),
    m("Super", ModifierKey::Super, 1.5),
    m("AltGr", ModifierKey::AltGraph, 1.5),
    n("Menu", Named::ContextMenu, 1.5),
];

static NAVIGATION_ROW: [KeyDef; 10] = [
    n("Ins", Named::Insert, 1.0),
    n("Home", Named::Home, 1.0),
    n("PgUp", Named::PageUp, 1.0),
    n("←", Named::ArrowLeft, 1.0),
    n("↑", Named::ArrowUp, 1.0),
    n("↓", Named::ArrowDown, 1.0),
    n("→", Named::ArrowRight, 1.0),
    n("PgDn", Named::PageDown, 1.0),
    n("End", Named::End, 1.0),
    n("Del", Named::Delete, 1.0),
];

static SYSTEM_ROW: [KeyDef; 4] = [
    n("PrtSc", Named::PrintScreen, 1.5),
    n("ScrLk", Named::ScrollLock, 1.5),
    n("Pause", Named::Pause, 1.5),
    n("NumLk", Named::NumLock, 1.5),
];

// Medium/Large lay out as three side-by-side blocks — main alphanumeric,
// nav/arrow cluster, numpad — each a `block()` of 6 rows so they line up
// vertically the way they do on a physical keyboard: the nav cluster's
// upper pair sits beside the number/qwerty rows, its arrow cluster sits
// beside the bottom/space rows, and the numpad spans the same six rows.
static FUNCTION_ROW: [KeyDef; 16] = [
    n("Esc", Named::Escape, 1.0),
    sp(0.5),
    n("F1", Named::F1, 1.0),
    n("F2", Named::F2, 1.0),
    n("F3", Named::F3, 1.0),
    n("F4", Named::F4, 1.0),
    sp(0.5),
    n("F5", Named::F5, 1.0),
    n("F6", Named::F6, 1.0),
    n("F7", Named::F7, 1.0),
    n("F8", Named::F8, 1.0),
    sp(0.5),
    n("F9", Named::F9, 1.0),
    n("F10", Named::F10, 1.0),
    n("F11", Named::F11, 1.0),
    n("F12", Named::F12, 1.0),
];

static NUMBER_ROW: [KeyDef; 14] = [
    c("`", 1.0),
    c("1", 1.0),
    c("2", 1.0),
    c("3", 1.0),
    c("4", 1.0),
    c("5", 1.0),
    c("6", 1.0),
    c("7", 1.0),
    c("8", 1.0),
    c("9", 1.0),
    c("0", 1.0),
    c("-", 1.0),
    c("=", 1.0),
    n("Backspace", Named::Backspace, 2.0),
];

static QWERTY_ROW: [KeyDef; 14] = [
    n("Tab", Named::Tab, 1.5),
    c("q", 1.0),
    c("w", 1.0),
    c("e", 1.0),
    c("r", 1.0),
    c("t", 1.0),
    c("y", 1.0),
    c("u", 1.0),
    c("i", 1.0),
    c("o", 1.0),
    c("p", 1.0),
    c("[", 1.0),
    c("]", 1.0),
    c("\\", 1.5),
];

static HOME_ROW: [KeyDef; 13] = [
    n("Caps", Named::CapsLock, 1.75),
    c("a", 1.0),
    c("s", 1.0),
    c("d", 1.0),
    c("f", 1.0),
    c("g", 1.0),
    c("h", 1.0),
    c("j", 1.0),
    c("k", 1.0),
    c("l", 1.0),
    c(";", 1.0),
    c("'", 1.0),
    n("Enter", Named::Enter, 2.25),
];

static BOTTOM_ROW: [KeyDef; 12] = [
    m("Shift", ModifierKey::Shift, 2.25),
    c("z", 1.0),
    c("x", 1.0),
    c("c", 1.0),
    c("v", 1.0),
    c("b", 1.0),
    c("n", 1.0),
    c("m", 1.0),
    c(",", 1.0),
    c(".", 1.0),
    c("/", 1.0),
    m("Shift", ModifierKey::Shift, 1.75),
];

static SPACE_ROW: [KeyDef; 7] = [
    m("Ctrl", ModifierKey::Control, 1.5),
    m("Super", ModifierKey::Super, 1.5),
    m("Alt", ModifierKey::Alt, 1.5),
    n("Space", Named::Space, 6.0),
    m("AltGr", ModifierKey::AltGraph, 1.5),
    n("Menu", Named::ContextMenu, 1.5),
    m("Ctrl", ModifierKey::Control, 1.5),
];

static NUMPAD_ROW_1: [KeyDef; 4] = [
    n("NumLk", Named::NumLock, 1.0),
    ks("/", KP_DIVIDE, 1.0),
    ks("*", KP_MULTIPLY, 1.0),
    ks("-", KP_SUBTRACT, 1.0),
];

static NUMPAD_ROW_2: [KeyDef; 4] = [
    ks("7", KP_7, 1.0),
    ks("8", KP_8, 1.0),
    ks("9", KP_9, 1.0),
    ks("+", KP_ADD, 1.0),
];

static NUMPAD_ROW_3: [KeyDef; 4] = [
    ks("4", KP_4, 1.0),
    ks("5", KP_5, 1.0),
    ks("6", KP_6, 1.0),
    sp(1.0),
];

static NUMPAD_ROW_4: [KeyDef; 4] = [
    ks("1", KP_1, 1.0),
    ks("2", KP_2, 1.0),
    ks("3", KP_3, 1.0),
    ks("Enter", KP_ENTER, 1.0),
];

static NUMPAD_ROW_5: [KeyDef; 3] = [ks("0", KP_0, 2.0), ks(".", KP_DECIMAL, 1.0), sp(1.0)];
static NUMPAD_ROW_6: [KeyDef; 4] = [sp(1.0), sp(1.0), sp(1.0), sp(1.0)];

// The nav/arrow cluster block, to the right of the main block — six rows so
// it lines up with `MAIN_ROWS` and `NUMPAD_ROWS` below.
static SIDE_SYSTEM_ROW: [KeyDef; 3] = [
    n("PrtSc", Named::PrintScreen, 1.0),
    n("ScrLk", Named::ScrollLock, 1.0),
    n("Pause", Named::Pause, 1.0),
];

static NAV_UPPER_ROW: [KeyDef; 3] = [
    n("Ins", Named::Insert, 1.0),
    n("Home", Named::Home, 1.0),
    n("PgUp", Named::PageUp, 1.0),
];

static NAV_LOWER_ROW: [KeyDef; 3] = [
    n("Del", Named::Delete, 1.0),
    n("End", Named::End, 1.0),
    n("PgDn", Named::PageDown, 1.0),
];

static BLANK_ROW_3: [KeyDef; 3] = [sp(1.0), sp(1.0), sp(1.0)];

static ARROW_UPPER_ROW: [KeyDef; 3] = [sp(1.0), n("↑", Named::ArrowUp, 1.0), sp(1.0)];

static ARROW_LOWER_ROW: [KeyDef; 3] = [
    n("←", Named::ArrowLeft, 1.0),
    n("↓", Named::ArrowDown, 1.0),
    n("→", Named::ArrowRight, 1.0),
];

static SMALL_ROWS: [&[KeyDef]; 3] = [&MODIFIER_ROW, &NAVIGATION_ROW, &SYSTEM_ROW];

static MAIN_ROWS: [&[KeyDef]; 6] = [
    &FUNCTION_ROW,
    &NUMBER_ROW,
    &QWERTY_ROW,
    &HOME_ROW,
    &BOTTOM_ROW,
    &SPACE_ROW,
];

static SIDE_ROWS: [&[KeyDef]; 6] = [
    &SIDE_SYSTEM_ROW,
    &NAV_UPPER_ROW,
    &NAV_LOWER_ROW,
    &BLANK_ROW_3,
    &ARROW_UPPER_ROW,
    &ARROW_LOWER_ROW,
];

static NUMPAD_ROWS: [&[KeyDef]; 6] = [
    &NUMPAD_ROW_1,
    &NUMPAD_ROW_2,
    &NUMPAD_ROW_3,
    &NUMPAD_ROW_4,
    &NUMPAD_ROW_5,
    &NUMPAD_ROW_6,
];

/// Pixels per width "unit" — the main block is 14 units wide, the side
/// (nav/arrow) block 3, and the numpad block 4, matching a physical
/// keyboard's proportions.
const UNIT_PX: f32 = 45.0;
const MAIN_BLOCK_WIDTH: f32 = UNIT_PX * 14.0;
const SIDE_BLOCK_WIDTH: f32 = UNIT_PX * 3.0;
const NUMPAD_BLOCK_WIDTH: f32 = UNIT_PX * 4.0;
/// Every key button (and blank spacer) is this tall, so rows across
/// different blocks line up with each other.
const KEY_HEIGHT: f32 = 34.0;

pub fn view(size: KeyboardSize, state: &State) -> Element<'_, Message> {
    match size {
        KeyboardSize::Small => small_view(state),
        KeyboardSize::Medium => full_view(state, false),
        KeyboardSize::Large => full_view(state, true),
    }
}

fn small_view(state: &State) -> Element<'_, Message> {
    let mut column = Column::new().spacing(2).padding(4);
    for &row in &SMALL_ROWS {
        column = column.push(key_row(row, state));
    }
    container(column).style(container::bordered_box).into()
}

/// A full keyboard shape: the main alphanumeric block, a nav/arrow cluster
/// to its right, and (for the large layout) a numpad further right —
/// mirroring where these clusters sit on a physical keyboard.
fn full_view(state: &State, with_numpad: bool) -> Element<'_, Message> {
    let mut row = Row::new()
        .spacing(6)
        .push(block(MAIN_BLOCK_WIDTH, &MAIN_ROWS, state))
        .push(block(SIDE_BLOCK_WIDTH, &SIDE_ROWS, state));
    if with_numpad {
        row = row.push(block(NUMPAD_BLOCK_WIDTH, &NUMPAD_ROWS, state));
    }
    container(row)
        .style(container::bordered_box)
        .padding(4)
        .into()
}

fn block<'a>(width: f32, rows: &[&[KeyDef]], state: &State) -> Element<'a, Message> {
    let mut column = Column::new().spacing(2).width(Length::Fixed(width));
    for &row in rows {
        column = column.push(key_row(row, state));
    }
    column.into()
}

fn key_row<'a>(defs: &[KeyDef], state: &State) -> Element<'a, Message> {
    let mut row = Row::new().spacing(2).width(Length::Fill);
    for def in defs {
        row = row.push(key_button(def, state));
    }
    row.into()
}

fn key_button<'a>(def: &KeyDef, state: &State) -> Element<'a, Message> {
    let units = (def.width * 4.0).round() as u16;
    let height = Length::Fixed(KEY_HEIGHT);
    match def.key {
        Key::Spacer => Space::new()
            .width(Length::FillPortion(units))
            .height(height)
            .into(),
        Key::Modifier(modifier) => {
            let style = match latch(state, modifier) {
                LatchState::Off => button::subtle,
                LatchState::Latched => button::primary,
                LatchState::Locked => button::success,
            };
            button(text(def.label))
                .width(Length::FillPortion(units))
                .height(height)
                .style(style)
                .on_press(Message::Modifier(modifier))
                .into()
        }
        Key::Char => {
            let character = def
                .label
                .chars()
                .next()
                .expect("Key::Char label is a single character");
            button(text(def.label))
                .width(Length::FillPortion(units))
                .height(height)
                .style(button::subtle)
                .on_press(Message::Tap(character_keysym(character)))
                .into()
        }
        Key::Named(named) => button(text(def.label))
            .width(Length::FillPortion(units))
            .height(height)
            .style(button::subtle)
            .on_press_maybe(named_keysym(named).map(Message::Tap))
            .into(),
        Key::Keysym(keysym) => button(text(def.label))
            .width(Length::FillPortion(units))
            .height(height)
            .style(button::subtle)
            .on_press(Message::Tap(keysym))
            .into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_tap_sends_down_then_up() {
        let mut state = State::default();
        let events = update(&mut state, Message::Tap(0x61));
        assert_eq!(events, vec![(0x61, true), (0x61, false)]);
    }

    #[test]
    fn modifier_cycles_off_latched_locked_off() {
        let mut state = State::default();
        let keysym = ModifierKey::Control.keysym();

        let events = update(&mut state, Message::Modifier(ModifierKey::Control));
        assert_eq!(events, vec![(keysym, true)]);
        assert_eq!(latch(&state, ModifierKey::Control), LatchState::Latched);

        let events = update(&mut state, Message::Modifier(ModifierKey::Control));
        assert!(events.is_empty());
        assert_eq!(latch(&state, ModifierKey::Control), LatchState::Locked);

        let events = update(&mut state, Message::Modifier(ModifierKey::Control));
        assert_eq!(events, vec![(keysym, false)]);
        assert_eq!(latch(&state, ModifierKey::Control), LatchState::Off);
    }

    #[test]
    fn latched_modifier_releases_after_the_next_tap() {
        let mut state = State::default();
        let ctrl = ModifierKey::Control.keysym();

        let _ = update(&mut state, Message::Modifier(ModifierKey::Control));
        assert_eq!(latch(&state, ModifierKey::Control), LatchState::Latched);

        let events = update(&mut state, Message::Tap(0x63));
        assert_eq!(events, vec![(0x63, true), (0x63, false), (ctrl, false)]);
        assert_eq!(latch(&state, ModifierKey::Control), LatchState::Off);
    }

    #[test]
    fn locked_modifier_stays_down_across_taps() {
        let mut state = State::default();
        let _ = update(&mut state, Message::Modifier(ModifierKey::Shift));
        let _ = update(&mut state, Message::Modifier(ModifierKey::Shift));
        assert_eq!(latch(&state, ModifierKey::Shift), LatchState::Locked);

        let events = update(&mut state, Message::Tap(0x61));
        assert_eq!(events, vec![(0x61, true), (0x61, false)]);
        assert_eq!(latch(&state, ModifierKey::Shift), LatchState::Locked);
    }
}
