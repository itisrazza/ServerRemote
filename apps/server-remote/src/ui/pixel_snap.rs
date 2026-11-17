//! Snaps the whole layout to the physical pixel grid.
//!
//! iced lays widgets out in fractional logical pixels: centring, text
//! measurements and fill portions all leave edges partway across a pixel.
//! At 2x scale that's barely visible, but at 1x (or 1.25x, 1.5x) borders and
//! icons smear across two pixels and look fuzzy. Wrapping the whole view in
//! [`pixel_snap`] rounds every layout node's edges, including in overlays
//! such as dropdown menus, to the nearest physical pixel.
//!
//! Edges are rounded in window coordinates, so the wrapper must be the root
//! of the view: it assumes it sits at the window's origin.

use iced::advanced::widget::{Operation, Tree, Widget, tree};
use iced::advanced::{Clipboard, Layout, Shell, layout, overlay, renderer};
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector, mouse};

pub fn pixel_snap<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    scale_factor: f32,
) -> Element<'a, Message> {
    Element::new(PixelSnap {
        content: content.into(),
        scale_factor,
    })
}

/// Rounds the node's edges to the pixel grid, given where its parent sits in
/// window coordinates before (`parent`) and after (`snapped_parent`)
/// snapping. Both edges round independently, so neighbours that touch keep
/// touching, with no gaps or overlaps.
fn snap(
    node: &layout::Node,
    parent: Point,
    snapped_parent: Point,
    scale_factor: f32,
) -> layout::Node {
    let round = |value: f32| (value * scale_factor).round() / scale_factor;
    let bounds = node.bounds() + Vector::new(parent.x, parent.y);
    let top_left = Point::new(round(bounds.x), round(bounds.y));
    let bottom_right = Point::new(
        round(bounds.x + bounds.width),
        round(bounds.y + bounds.height),
    );
    let children = node
        .children()
        .iter()
        .map(|child| snap(child, bounds.position(), top_left, scale_factor))
        .collect();
    layout::Node::with_children(
        Size::new(bottom_right.x - top_left.x, bottom_right.y - top_left.y),
        children,
    )
    .move_to(top_left - Vector::new(snapped_parent.x, snapped_parent.y))
}

fn snap_root(node: &layout::Node, scale_factor: f32) -> layout::Node {
    snap(node, Point::ORIGIN, Point::ORIGIN, scale_factor)
}

struct PixelSnap<'a, Message> {
    content: Element<'a, Message>,
    scale_factor: f32,
}

// Delegates everything, including the widget tree, straight to the content,
// so wrapping changes nothing but where edges land.
impl<Message> Widget<Message, iced::Theme, iced::Renderer> for PixelSnap<'_, Message> {
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
        let node = self.content.as_widget_mut().layout(tree, renderer, limits);
        snap_root(&node, self.scale_factor)
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
        self.content
            .as_widget()
            .mouse_interaction(tree, layout, cursor, viewport, renderer)
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        let scale_factor = self.scale_factor;
        self.content
            .as_widget_mut()
            .overlay(tree, layout, renderer, viewport, translation)
            .map(|content| SnapOverlay::wrap(content, scale_factor))
    }
}

struct SnapOverlay<'a, Message> {
    content: overlay::Element<'a, Message, iced::Theme, iced::Renderer>,
    scale_factor: f32,
}

impl<'a, Message: 'a> SnapOverlay<'a, Message> {
    fn wrap(
        content: overlay::Element<'a, Message, iced::Theme, iced::Renderer>,
        scale_factor: f32,
    ) -> overlay::Element<'a, Message, iced::Theme, iced::Renderer> {
        overlay::Element::new(Box::new(SnapOverlay {
            content,
            scale_factor,
        }))
    }
}

// Overlay layouts are already in window coordinates.
impl<Message> overlay::Overlay<Message, iced::Theme, iced::Renderer> for SnapOverlay<'_, Message> {
    fn layout(&mut self, renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let node = self.content.as_overlay_mut().layout(renderer, bounds);
        snap_root(&node, self.scale_factor)
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
        self.content
            .as_overlay()
            .mouse_interaction(layout, cursor, renderer)
    }

    fn overlay<'b>(
        &'b mut self,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
    ) -> Option<overlay::Element<'b, Message, iced::Theme, iced::Renderer>> {
        let scale_factor = self.scale_factor;
        self.content
            .as_overlay_mut()
            .overlay(layout, renderer)
            .map(|content| SnapOverlay::wrap(content, scale_factor))
    }

    fn index(&self) -> f32 {
        self.content.as_overlay().index()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(x: f32, y: f32, width: f32, height: f32, children: Vec<layout::Node>) -> layout::Node {
        layout::Node::with_children(Size::new(width, height), children).move_to(Point::new(x, y))
    }

    #[test]
    fn edges_land_on_physical_pixels() {
        let cases = [
            // (scale factor, node, expected window-space bounds)
            (1.0, (10.4, 3.5, 20.3, 16.9), (10.0, 4.0, 21.0, 16.0)),
            (2.0, (10.4, 3.3, 20.3, 16.9), (10.5, 3.5, 20.0, 16.5)),
            (
                1.5,
                (10.0, 1.0, 5.0, 5.0),
                (10.0, 4.0 / 3.0, 16.0 / 3.0, 14.0 / 3.0),
            ),
            (1.0, (2.0, 2.0, 8.0, 8.0), (2.0, 2.0, 8.0, 8.0)),
        ];
        for (scale_factor, (x, y, width, height), expected) in cases {
            let bounds = snap_root(&node(x, y, width, height, vec![]), scale_factor).bounds();
            let actual = (bounds.x, bounds.y, bounds.width, bounds.height);
            let close = |a: f32, b: f32| (a - b).abs() < 1e-4;
            assert!(
                close(actual.0, expected.0)
                    && close(actual.1, expected.1)
                    && close(actual.2, expected.2)
                    && close(actual.3, expected.3),
                "scale {scale_factor}: {actual:?} != {expected:?}",
            );
        }
    }

    #[test]
    fn children_snap_in_window_coordinates() {
        // Each child is offset by half a pixel from a parent that's also off
        // by a fraction, so only the combined position is whole-pixel aligned.
        let root = node(
            0.3,
            0.0,
            100.0,
            20.0,
            vec![
                node(0.2, 0.0, 10.0, 10.0, vec![]),
                node(10.2, 0.0, 10.0, 10.0, vec![]),
            ],
        );
        let snapped = snap_root(&root, 1.0);
        let window_x: Vec<f32> = snapped
            .children()
            .iter()
            .map(|child| snapped.bounds().x + child.bounds().x)
            .collect();
        assert_eq!(snapped.bounds().x, 0.0);
        assert_eq!(window_x, vec![1.0, 11.0]);
        // Neighbours that touched still touch.
        let first = snapped.children()[0].bounds();
        assert_eq!(first.x + first.width, snapped.children()[1].bounds().x);
    }
}
