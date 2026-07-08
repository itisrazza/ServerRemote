use iced::{
    Element, padding,
    widget::{Container, container},
};

pub fn tooltip_box<'state, Message>(
    content: impl Into<Element<'state, Message>>,
) -> Container<'state, Message> {
    Container::new(content)
        .style(container::bordered_box)
        .padding(padding::all(4).horizontal(8))
}
