//! A small custom widget that captures single-finger drag gestures (mouse or
//! touch) and turns them into relative-cursor movement, plus an explicit
//! Left/Middle/Right button row for clicks.
//!
//! This is an interim stand-in for real pointer-lock/mouse-capture support:
//! `PointerMode::Relative` isn't implemented anywhere yet, so instead this
//! widget tracks its own virtual cursor position and reports it as absolute
//! framebuffer coordinates through the same path real absolute-pointer input
//! already uses.

use crate::i18n::fl;
use iced::advanced::widget::Widget;
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::{self, Clipboard, Layout, Renderer as _, Shell, layout, renderer};
use iced::widget::{Column, Row, button, text};
use iced::{Border, Element, Event, Length, Point, Rectangle, Size, Vector, mouse, touch};
use server_remote_core::PointerButton;

/// The virtual cursor position, in framebuffer pixel space. Persists across
/// `view()` rebuilds so drags feel continuous.
#[derive(Default)]
pub struct State {
    cursor: Option<Point>,
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    /// A drag moved by this much, in the trackpad surface's own pixels.
    Drag { dx: f32, dy: f32 },
    /// A full click (press + release) of the given button — either a tap on
    /// the surface or a press of one of the explicit buttons below it.
    Click(PointerButton),
}

pub enum Effect {
    Move(Point),
    Click(PointerButton, bool),
}

/// Applies `message`, returning the effects to send to the session. `fb` is
/// the current remote framebuffer size, used to seed and clamp the virtual
/// cursor.
pub fn update(state: &mut State, message: Message, fb: (f32, f32)) -> Vec<Effect> {
    match message {
        Message::Drag { dx, dy } => {
            let (fb_w, fb_h) = fb;
            let cursor = state
                .cursor
                .get_or_insert_with(|| Point::new(fb_w / 2.0, fb_h / 2.0));
            cursor.x = (cursor.x + dx).clamp(0.0, (fb_w - 1.0).max(0.0));
            cursor.y = (cursor.y + dy).clamp(0.0, (fb_h - 1.0).max(0.0));
            vec![Effect::Move(*cursor)]
        }
        Message::Click(button) => vec![Effect::Click(button, true), Effect::Click(button, false)],
    }
}

pub fn view() -> Element<'static, Message> {
    Column::new()
        .spacing(4)
        .width(Length::Fixed(TRACKPAD_WIDTH))
        .push(Element::new(TrackpadSurface))
        .push(
            Row::new()
                .spacing(4)
                .push(click_button(fl!("trackpad-left"), PointerButton::Left))
                .push(click_button(fl!("trackpad-middle"), PointerButton::Middle))
                .push(click_button(fl!("trackpad-right"), PointerButton::Right)),
        )
        .into()
}

fn click_button<'a>(label: String, button_kind: PointerButton) -> Element<'a, Message> {
    button(text(label))
        .style(button::subtle)
        .width(Length::Fill)
        .on_press(Message::Click(button_kind))
        .into()
}

/// Fixed at a 3:2 aspect ratio, like a physical trackpad.
const TRACKPAD_HEIGHT: f32 = 200.0;
const TRACKPAD_WIDTH: f32 = TRACKPAD_HEIGHT * 1.5;
/// Below this accumulated movement, a press-then-release is a tap-to-click
/// rather than a deliberate drag.
const TAP_THRESHOLD: f32 = 4.0;

struct DragState {
    last_position: Point,
    total_movement: f32,
}

#[derive(Default)]
struct WidgetState {
    dragging: Option<DragState>,
    active_finger: Option<touch::Finger>,
}

impl WidgetState {
    fn begin_drag(&mut self, position: Point) {
        self.dragging = Some(DragState {
            last_position: position,
            total_movement: 0.0,
        });
    }

    fn last_position(&self) -> Option<Point> {
        self.dragging.as_ref().map(|drag| drag.last_position)
    }

    fn advance_drag(&mut self, position: Point) -> Option<(f32, f32)> {
        let drag = self.dragging.as_mut()?;
        let dx = position.x - drag.last_position.x;
        let dy = position.y - drag.last_position.y;
        drag.last_position = position;
        drag.total_movement += dx.hypot(dy);
        Some((dx, dy))
    }

    /// Ends the current drag, returning `true` if it was short enough to
    /// count as a tap-to-click rather than a deliberate drag.
    fn end_drag(&mut self) -> bool {
        self.dragging
            .take()
            .is_some_and(|drag| drag.total_movement < TAP_THRESHOLD)
    }
}

fn local_point(bounds: Rectangle, position: Point) -> Option<Point> {
    bounds
        .contains(position)
        .then(|| position - Vector::new(bounds.x, bounds.y))
}

/// The pure decision logic behind [`Widget::update`], kept separate so it's
/// testable without constructing a live `iced::Renderer`. Handles both mouse
/// and touch so it works with a real mouse during development and with
/// touch on a tablet.
fn interpret_event(
    state: &mut WidgetState,
    bounds: Rectangle,
    cursor: mouse::Cursor,
    event: &Event,
) -> Option<Message> {
    match event {
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
            if state.active_finger.is_none() =>
        {
            let position = cursor.position_in(bounds)?;
            state.begin_drag(position);
            None
        }
        Event::Mouse(mouse::Event::CursorMoved { .. })
            if state.active_finger.is_none() && state.dragging.is_some() =>
        {
            let position = cursor
                .position_in(bounds)
                .or_else(|| state.last_position())?;
            state
                .advance_drag(position)
                .map(|(dx, dy)| Message::Drag { dx, dy })
        }
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            if state.active_finger.is_none() && state.dragging.is_some() =>
        {
            state
                .end_drag()
                .then_some(Message::Click(PointerButton::Left))
        }
        Event::Touch(touch::Event::FingerPressed { id, position }) if state.dragging.is_none() => {
            let local = local_point(bounds, *position)?;
            state.active_finger = Some(*id);
            state.begin_drag(local);
            None
        }
        Event::Touch(touch::Event::FingerMoved { id, position })
            if state.active_finger == Some(*id) =>
        {
            let local = local_point(bounds, *position).or_else(|| state.last_position())?;
            state
                .advance_drag(local)
                .map(|(dx, dy)| Message::Drag { dx, dy })
        }
        Event::Touch(
            touch::Event::FingerLifted { id, .. } | touch::Event::FingerLost { id, .. },
        ) if state.active_finger == Some(*id) => {
            state.active_finger = None;
            state
                .end_drag()
                .then_some(Message::Click(PointerButton::Left))
        }
        _ => None,
    }
}

struct TrackpadSurface;

impl Widget<Message, iced::Theme, iced::Renderer> for TrackpadSurface {
    fn size(&self) -> Size<Length> {
        Size {
            width: Length::Fixed(TRACKPAD_WIDTH),
            height: Length::Fixed(TRACKPAD_HEIGHT),
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        advanced::layout::atomic(
            limits,
            Length::Fixed(TRACKPAD_WIDTH),
            Length::Fixed(TRACKPAD_HEIGHT),
        )
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<WidgetState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(WidgetState::default())
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<WidgetState>();

        if let Some(message) = interpret_event(state, bounds, cursor, event) {
            shell.publish(message);
            shell.capture_event();
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let palette = theme.extended_palette();
        renderer.fill_quad(
            renderer::Quad {
                bounds: layout.bounds(),
                border: Border {
                    width: 1.0,
                    radius: 4.0.into(),
                    color: palette.background.weak.color,
                },
                ..renderer::Quad::default()
            },
            palette.background.weakest.color,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOUNDS: Rectangle = Rectangle {
        x: 100.0,
        y: 50.0,
        width: 300.0,
        height: 160.0,
    };

    #[test]
    fn drag_below_threshold_is_a_tap() {
        let mut state = WidgetState::default();
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let cursor = mouse::Cursor::Available(Point::new(200.0, 100.0));
        assert!(interpret_event(&mut state, BOUNDS, cursor, &press).is_none());

        let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        match interpret_event(&mut state, BOUNDS, cursor, &release) {
            Some(Message::Click(PointerButton::Left)) => {}
            other => panic!("expected a tap-to-click message, got {other:?}"),
        }
    }

    #[test]
    fn drag_above_threshold_emits_deltas_and_no_click_on_release() {
        let mut state = WidgetState::default();
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let start = mouse::Cursor::Available(Point::new(200.0, 100.0));
        assert!(interpret_event(&mut state, BOUNDS, start, &press).is_none());

        let moved = Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(250.0, 100.0),
        });
        let end = mouse::Cursor::Available(Point::new(250.0, 100.0));
        match interpret_event(&mut state, BOUNDS, end, &moved) {
            Some(Message::Drag { dx, dy }) => {
                assert_eq!((dx, dy), (50.0, 0.0));
            }
            other => panic!("expected a drag message, got {other:?}"),
        }

        let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        assert!(interpret_event(&mut state, BOUNDS, end, &release).is_none());
    }

    #[test]
    fn touch_drag_is_tracked_by_finger_id() {
        let mut state = WidgetState::default();
        let finger = touch::Finger(1);
        let pressed = Event::Touch(touch::Event::FingerPressed {
            id: finger,
            position: Point::new(150.0, 100.0),
        });
        let cursor = mouse::Cursor::Unavailable;
        assert!(interpret_event(&mut state, BOUNDS, cursor, &pressed).is_none());

        let moved = Event::Touch(touch::Event::FingerMoved {
            id: finger,
            position: Point::new(160.0, 100.0),
        });
        match interpret_event(&mut state, BOUNDS, cursor, &moved) {
            Some(Message::Drag { dx, dy }) => assert_eq!((dx, dy), (10.0, 0.0)),
            other => panic!("expected a drag message, got {other:?}"),
        }

        let lifted = Event::Touch(touch::Event::FingerLifted {
            id: finger,
            position: Point::new(160.0, 100.0),
        });
        assert!(interpret_event(&mut state, BOUNDS, cursor, &lifted).is_none());
    }

    #[test]
    fn update_drag_seeds_cursor_at_framebuffer_center_and_clamps() {
        let mut state = State::default();
        let fb = (800.0, 600.0);

        let effects = update(&mut state, Message::Drag { dx: 10.0, dy: 0.0 }, fb);
        match effects.as_slice() {
            [Effect::Move(point)] => assert_eq!(*point, Point::new(410.0, 300.0)),
            other => panic!("expected one move effect, got {} effects", other.len()),
        }

        let effects = update(
            &mut state,
            Message::Drag {
                dx: -10_000.0,
                dy: 0.0,
            },
            fb,
        );
        match effects.as_slice() {
            [Effect::Move(point)] => assert_eq!(point.x, 0.0),
            other => panic!("expected one move effect, got {} effects", other.len()),
        }
    }

    #[test]
    fn update_click_produces_a_press_then_release_pair() {
        let mut state = State::default();
        let effects = update(
            &mut state,
            Message::Click(PointerButton::Right),
            (800.0, 600.0),
        );
        match effects.as_slice() {
            [
                Effect::Click(PointerButton::Right, true),
                Effect::Click(PointerButton::Right, false),
            ] => {}
            other => panic!("expected a press/release pair, got {} effects", other.len()),
        }
    }
}
