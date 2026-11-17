//! Keeps the normal arrow cursor over clickable widgets.
//!
//! iced's buttons (and checkboxes, pick lists, ...) always ask for the
//! pointing-hand cursor, with no style option to turn it off. Wrapping the
//! whole view in [`arrow_cursor`] swaps that for the arrow everywhere,
//! including in overlays such as dropdown menus, while leaving every other
//! cursor (resize edges, text) alone.

use iced::advanced::widget::{Operation, Tree, Widget, tree};
use iced::advanced::{Clipboard, Layout, Shell, layout, overlay, renderer};
use iced::{Element, Event, Length, Rectangle, Size, Vector, mouse};

pub fn arrow_cursor<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    Element::new(ArrowCursor {
        content: content.into(),
    })
}

fn arrow(interaction: mouse::Interaction) -> mouse::Interaction {
    match interaction {
        mouse::Interaction::Pointer => mouse::Interaction::Idle,
        other => other,
    }
}

struct ArrowCursor<'a, Message> {
    content: Element<'a, Message>,
}

// Delegates everything, including the widget tree, straight to the content,
// so wrapping changes nothing but the cursor.
impl<Message> Widget<Message, iced::Theme, iced::Renderer> for ArrowCursor<'_, Message> {
    fn tag(&self) -> tree::Tag {
        self.content.as_widget().tag()
    }

    fn state(&self) -> tree::State {
        self.content.as_widget().state()
    }

    fn children(&self) -> Vec<Tree> {
        self.content.as_widget().children()
    }

    fn diff(&self, tree: &mut Tree) {
        self.content.as_widget().diff(tree);
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content.as_widget_mut().layout(tree, renderer, limits)
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(tree, layout, renderer, operation);
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
        self.content.as_widget_mut().update(
            tree, event, layout, cursor, renderer, clipboard, shell, viewport,
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
        self.content
            .as_widget()
            .draw(tree, renderer, theme, style, layout, cursor, viewport);
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        arrow(
            self.content
                .as_widget()
                .mouse_interaction(tree, layout, cursor, viewport, renderer),
        )
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
            .map(ArrowOverlay::wrap)
    }
}

struct ArrowOverlay<'a, Message> {
    content: overlay::Element<'a, Message, iced::Theme, iced::Renderer>,
}

impl<'a, Message: 'a> ArrowOverlay<'a, Message> {
    fn wrap(
        content: overlay::Element<'a, Message, iced::Theme, iced::Renderer>,
    ) -> overlay::Element<'a, Message, iced::Theme, iced::Renderer> {
        overlay::Element::new(Box::new(ArrowOverlay { content }))
    }
}

impl<Message> overlay::Overlay<Message, iced::Theme, iced::Renderer> for ArrowOverlay<'_, Message> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        self.content.as_overlay_mut().layout(renderer, bounds)
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.content
            .as_overlay()
            .draw(renderer, theme, style, layout, cursor);
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_overlay_mut()
            .operate(layout, renderer, operation);
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
        self.content
            .as_overlay_mut()
            .update(event, layout, cursor, renderer, clipboard, shell);
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        arrow(
            self.content
                .as_overlay()
                .mouse_interaction(layout, cursor, renderer),
        )
    }

    fn overlay<'b>(
        &'b mut self,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        self.content
            .as_overlay_mut()
            .overlay(layout, renderer)
            .map(ArrowOverlay::wrap)
    }

    fn index(&self) -> f32 {
        self.content.as_overlay().index()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_pointing_hand_is_replaced() {
        let cases = [
            (mouse::Interaction::Pointer, mouse::Interaction::Idle),
            (mouse::Interaction::Idle, mouse::Interaction::Idle),
            (mouse::Interaction::None, mouse::Interaction::None),
            (mouse::Interaction::Text, mouse::Interaction::Text),
            (
                mouse::Interaction::ResizingHorizontally,
                mouse::Interaction::ResizingHorizontally,
            ),
        ];
        for (interaction, expected) in cases {
            assert_eq!(arrow(interaction), expected, "{interaction:?}");
        }
    }
}
