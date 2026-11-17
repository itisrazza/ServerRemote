//! Anchors a popup element (a dropdown menu, a submenu) to another element.
//!
//! The popup is drawn in an overlay so it floats above everything else. Whether
//! it's shown is owned by the caller: pass `Some(popup)` to show it. Clicking
//! outside both the popup and its anchor, or pressing Escape, publishes the
//! dismiss message and swallows the event so it doesn't reach the widgets
//! underneath (most importantly, the remote display). Clicks on the anchor pass
//! through so it can toggle the popup itself.
//!
//! A nested popup (a submenu) passes no dismiss message: it only swallows
//! clicks on itself and leaves outside clicks to its parent, so clicking an
//! entry in the parent menu still reaches that entry.

use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Clipboard, Layout, Shell, layout, overlay, renderer, widget};
use iced::keyboard::{self, key::Named};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector, mouse, touch};

const GAP: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Below the anchor, left edges aligned.
    Below,
    /// To the right of the anchor, top edges aligned; flips left if it would
    /// overflow the window.
    Right,
}

pub struct Popover<'a, Message> {
    anchor: Element<'a, Message>,
    popup: Option<Element<'a, Message>>,
    placement: Placement,
    on_dismiss: Option<Message>,
}

pub fn popover<'a, Message>(
    anchor: impl Into<Element<'a, Message>>,
    popup: Option<Element<'a, Message>>,
    placement: Placement,
    on_dismiss: Option<Message>,
) -> Popover<'a, Message> {
    Popover {
        anchor: anchor.into(),
        popup,
        placement,
        on_dismiss,
    }
}

impl<Message: Clone> Widget<Message, iced::Theme, iced::Renderer> for Popover<'_, Message> {
    fn children(&self) -> Vec<Tree> {
        std::iter::once(&self.anchor)
            .chain(&self.popup)
            .map(Tree::new)
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let widgets: Vec<_> = std::iter::once(&self.anchor)
            .chain(&self.popup)
            .map(Element::as_widget)
            .collect();
        tree.diff_children(&widgets);
    }

    fn size(&self) -> Size<Length> {
        self.anchor.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.anchor.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.anchor
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.anchor.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.anchor.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.anchor.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.anchor
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        let (anchor_tree, popup_tree) = tree.children.split_at_mut(1);

        let anchor_overlay = self.anchor.as_widget_mut().overlay(
            &mut anchor_tree[0],
            layout,
            renderer,
            viewport,
            translation,
        );

        let popup_overlay = self
            .popup
            .as_mut()
            .zip(popup_tree.first_mut())
            .map(|(popup, tree)| {
                overlay::Element::new(Box::new(PopupOverlay {
                    popup,
                    tree,
                    anchor: layout.bounds() + translation,
                    placement: self.placement,
                    on_dismiss: self.on_dismiss.as_ref(),
                }))
            });

        let children: Vec<_> = anchor_overlay.into_iter().chain(popup_overlay).collect();
        (!children.is_empty()).then(|| overlay::Group::with_children(children).overlay())
    }
}

impl<'a, Message: Clone + 'a> From<Popover<'a, Message>> for Element<'a, Message> {
    fn from(popover: Popover<'a, Message>) -> Self {
        Element::new(popover)
    }
}

struct PopupOverlay<'a, 'b, Message> {
    popup: &'b mut Element<'a, Message>,
    tree: &'b mut Tree,
    anchor: Rectangle,
    placement: Placement,
    on_dismiss: Option<&'b Message>,
}

/// Where the popup's top-left corner goes, kept inside the window.
fn popup_position(anchor: Rectangle, popup: Size, window: Size, placement: Placement) -> Point {
    let (x, y) = match placement {
        Placement::Below => (anchor.x, anchor.y + anchor.height + GAP),
        Placement::Right => {
            let right = anchor.x + anchor.width + GAP;
            let x = if right + popup.width > window.width {
                anchor.x - popup.width - GAP
            } else {
                right
            };
            (x, anchor.y)
        }
    };
    Point::new(
        x.min(window.width - popup.width).max(0.0),
        y.min(window.height - popup.height).max(0.0),
    )
}

impl<Message: Clone> overlay::Overlay<Message, iced::Theme, iced::Renderer>
    for PopupOverlay<'_, '_, Message>
{
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let node = self.popup.as_widget_mut().layout(
            self.tree,
            renderer,
            &layout::Limits::new(Size::ZERO, bounds),
        );
        let position = popup_position(self.anchor, node.size(), bounds, self.placement);
        node.move_to(position)
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.popup.as_widget().draw(
            self.tree,
            renderer,
            theme,
            style,
            layout,
            cursor,
            &layout.bounds(),
        );
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        self.popup.as_widget_mut().update(
            self.tree,
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &layout.bounds(),
        );
        if shell.is_event_captured() {
            return;
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(_))
            | Event::Touch(touch::Event::FingerPressed { .. }) => {
                if cursor.is_over(layout.bounds()) {
                    shell.capture_event();
                } else if !cursor.is_over(self.anchor) {
                    if let Some(on_dismiss) = self.on_dismiss {
                        shell.publish(on_dismiss.clone());
                        shell.capture_event();
                    }
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(Named::Escape),
                ..
            }) => {
                if let Some(on_dismiss) = self.on_dismiss {
                    shell.publish(on_dismiss.clone());
                    shell.capture_event();
                }
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if !cursor.is_over(layout.bounds()) {
            return mouse::Interaction::None;
        }
        self.popup
            .as_widget()
            .mouse_interaction(self.tree, layout, cursor, &layout.bounds(), renderer)
            .max(mouse::Interaction::Idle)
    }

    fn overlay<'c>(
        &'c mut self,
        layout: Layout<'c>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'c, Message, iced::Theme, iced::Renderer>> {
        self.popup.as_widget_mut().overlay(
            self.tree,
            layout,
            renderer,
            &layout.bounds(),
            Vector::ZERO,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: Size = Size::new(800.0, 600.0);
    const POPUP: Size = Size::new(200.0, 100.0);

    #[test]
    fn popup_positions() {
        let cases = [
            (
                "below, fits",
                Rectangle::new(Point::new(10.0, 10.0), Size::new(40.0, 20.0)),
                Placement::Below,
                Point::new(10.0, 34.0),
            ),
            (
                "below, clamped to the right edge",
                Rectangle::new(Point::new(700.0, 10.0), Size::new(40.0, 20.0)),
                Placement::Below,
                Point::new(600.0, 34.0),
            ),
            (
                "right, fits",
                Rectangle::new(Point::new(10.0, 50.0), Size::new(100.0, 20.0)),
                Placement::Right,
                Point::new(114.0, 50.0),
            ),
            (
                "right, flips to the left",
                Rectangle::new(Point::new(600.0, 50.0), Size::new(100.0, 20.0)),
                Placement::Right,
                Point::new(396.0, 50.0),
            ),
            (
                "right, clamped to the bottom edge",
                Rectangle::new(Point::new(10.0, 580.0), Size::new(100.0, 20.0)),
                Placement::Right,
                Point::new(114.0, 500.0),
            ),
        ];

        for (name, anchor, placement, expected) in cases {
            assert_eq!(
                popup_position(anchor, POPUP, WINDOW, placement),
                expected,
                "{name}"
            );
        }
    }
}
