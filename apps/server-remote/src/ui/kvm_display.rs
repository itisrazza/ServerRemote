//! A custom widget that renders the remote framebuffer through a persistent
//! `wgpu` texture, updated in place, instead of iced's `image` widget.
//!
//! `image::Handle::from_rgba` allocates a globally-unique, uncached id on
//! every call, so every `SessionEvent::Frame` forced a brand-new atlas
//! upload — the cost scaling with resolution and frame rate until it
//! contended with the render loop and showed up as visible strobing. This
//! widget instead keeps one GPU texture alive across frames and only
//! uploads to it when the frame actually changed.
//!
//! It also owns keyboard focus directly (via `operation::Focusable`),
//! replacing the previous window-level `keyboard::listen()` subscription
//! gated by whichever tab happened to be selected.

use std::sync::Arc;

use iced::advanced::widget::operation::{self, Operation};
use iced::advanced::widget::tree::{self, Tree};
use iced::advanced::widget::{self, Widget};
use iced::advanced::{self, Clipboard, Layout, Shell, layout, renderer};
use iced::widget::{center, responsive};
use iced::{Element, Event, Length, Point, Rectangle, Size, keyboard, mouse};
use iced_wgpu::primitive::{self, Renderer as PrimitiveRenderer};
use iced_wgpu::wgpu;
use iced_wgpu::wgpu::util::DeviceExt;
use server_remote_core::{PointerButton, PointerMode};

use super::connection::Message;

const SHADER: &str = r#"
struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@location(0) position: vec2<f32>, @location(1) uv: vec2<f32>) -> VertexOutput {
    var out: VertexOutput;
    out.position = vec4<f32>(position, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@group(0) @binding(0) var t_frame: texture_2d<f32>;
@group(0) @binding(1) var s_frame: sampler;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_frame, s_frame, in.uv);
}
"#;

// A full-bounds quad (triangle strip): top-left, top-right, bottom-left,
// bottom-right. The render pass's viewport/scissor is already set to this
// primitive's bounds (see `Primitive::draw`'s contract), so a fixed NDC quad
// lands exactly on the widget's on-screen rectangle without any per-frame
// vertex math.
const VERTICES: [[f32; 4]; 4] = [
    [-1.0, 1.0, 0.0, 0.0],
    [1.0, 1.0, 1.0, 0.0],
    [-1.0, -1.0, 0.0, 1.0],
    [1.0, -1.0, 1.0, 1.0],
];

const VERTEX_BUFFER_LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
    array_stride: 16,
    step_mode: wgpu::VertexStepMode::Vertex,
    attributes: &[
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 0,
            shader_location: 0,
        },
        wgpu::VertexAttribute {
            format: wgpu::VertexFormat::Float32x2,
            offset: 8,
            shader_location: 1,
        },
    ],
};

/// A displayed framebuffer: the pixel data plus enough identity to know
/// whether the GPU texture needs updating.
#[derive(Clone)]
pub(crate) struct DisplayFrame {
    /// Disambiguates tabs that coincidentally reach the same `generation`.
    pub(crate) tab_id: u64,
    /// Bumped once per real `SessionEvent::Frame`, not per `view()` call.
    pub(crate) generation: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Arc<[u8]>,
}

/// The framebuffer as an aspect-preserving, scaled display with pointer and
/// keyboard handling. `responsive` gives us the on-screen area, so the
/// widget can be sized to fit while preserving the framebuffer's aspect
/// ratio (letterboxing to fit, same as the old `image`-based view).
pub(crate) fn kvm_display(
    frame: DisplayFrame,
    pointer_mode: PointerMode,
    id: widget::Id,
) -> Element<'static, Message> {
    let fb = (frame.width.max(1) as f32, frame.height.max(1) as f32);

    responsive(move |area| {
        let (disp_w, disp_h) = letterboxed_size(area, fb);
        center(Element::new(KvmDisplay {
            frame: frame.clone(),
            pointer_mode,
            width: Length::Fixed(disp_w),
            height: Length::Fixed(disp_h),
            id: id.clone(),
        }))
        .into()
    })
    .into()
}

/// The scale factor that fits the framebuffer inside `area` while preserving
/// its aspect ratio.
fn fit_scale(area: Size, fb: (f32, f32)) -> f32 {
    (area.width / fb.0).min(area.height / fb.1)
}

/// The displayed size of the framebuffer inside `area`, preserving aspect
/// ratio and letterboxing to fit.
fn letterboxed_size(area: Size, fb: (f32, f32)) -> (f32, f32) {
    let scale = fit_scale(area, fb);
    (fb.0 * scale, fb.1 * scale)
}

/// Maps a point local to the widget's own bounds (already exactly the
/// letterboxed display rect, since the widget is sized to `letterboxed_size`)
/// to framebuffer pixel coordinates, clamped to the framebuffer bounds.
fn map_to_framebuffer(bounds_size: Size, fb: (f32, f32), point: Point) -> Point {
    let scale = fit_scale(bounds_size, fb);
    let x = (point.x / scale).clamp(0.0, fb.0 - 1.0);
    let y = (point.y / scale).clamp(0.0, fb.1 - 1.0);
    Point::new(x, y)
}

/// `Back`/`Forward` are passed through for connectors to decide on
/// themselves (see [`PointerButton`]'s docs) — only truly vendor-specific
/// codes (`Other`) have no sensible generic representation at all and are
/// dropped here.
fn translate_button(button: mouse::Button) -> Option<PointerButton> {
    match button {
        mouse::Button::Left => Some(PointerButton::Left),
        mouse::Button::Right => Some(PointerButton::Right),
        mouse::Button::Middle => Some(PointerButton::Middle),
        mouse::Button::Back => Some(PointerButton::Back),
        mouse::Button::Forward => Some(PointerButton::Forward),
        mouse::Button::Other(_) => None,
    }
}

/// The pure decision logic behind [`Widget::update`]: given the widget's
/// current focus state and a raw event, what (if any) message it produces,
/// mutating `state.focused` on left-button press/release the same way
/// `text_input` tracks its own focus. Kept separate from `Widget::update`
/// so it's testable without constructing a live `iced::Renderer`.
fn interpret_event(
    state: &mut KvmDisplayState,
    pointer_mode: PointerMode,
    fb: (f32, f32),
    bounds: Rectangle,
    cursor: mouse::Cursor,
    event: &Event,
) -> Option<Message> {
    match event {
        Event::Mouse(mouse_event) if pointer_mode == PointerMode::Absolute => match mouse_event {
            mouse::Event::ButtonPressed(button) => {
                let inside = cursor.position_in(bounds).is_some();
                if *button == mouse::Button::Left {
                    state.focused = inside;
                }
                inside
                    .then(|| translate_button(*button))
                    .flatten()
                    .map(|button| Message::Button {
                        button,
                        pressed: true,
                    })
            }
            mouse::Event::ButtonReleased(button) => {
                translate_button(*button).map(|button| Message::Button {
                    button,
                    pressed: false,
                })
            }
            mouse::Event::CursorMoved { .. } => cursor
                .position_in(bounds)
                .map(|local| Message::Pointer(map_to_framebuffer(bounds.size(), fb, local))),
            _ => None,
        },
        Event::Keyboard(keyboard_event) if state.focused => match keyboard_event {
            keyboard::Event::KeyPressed { modified_key, .. } => Some(Message::Key {
                key: modified_key.clone(),
                pressed: true,
            }),
            keyboard::Event::KeyReleased { modified_key, .. } => Some(Message::Key {
                key: modified_key.clone(),
                pressed: false,
            }),
            _ => None,
        },
        _ => None,
    }
}

/// Whether the cached `(tab_id, generation)` no longer matches the current
/// frame, meaning the texture needs a fresh upload.
fn needs_upload(current: (u64, u64), cached: Option<(u64, u64)>) -> bool {
    cached != Some(current)
}

/// Whether the framebuffer's dimensions changed, meaning the texture (and
/// its bind group) must be recreated before it can be written to.
fn needs_resize(current: (u32, u32), cached: (u32, u32)) -> bool {
    current != cached
}

#[derive(Default)]
struct KvmDisplayState {
    focused: bool,
}

impl operation::Focusable for KvmDisplayState {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

struct KvmDisplay {
    frame: DisplayFrame,
    pointer_mode: PointerMode,
    width: Length,
    height: Length,
    id: widget::Id,
}

impl<Theme> Widget<Message, Theme, iced::Renderer> for KvmDisplay {
    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        advanced::layout::atomic(limits, self.width, self.height)
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<KvmDisplayState>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(KvmDisplayState::default())
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &iced::Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<KvmDisplayState>();
        operation.focusable(Some(&self.id), layout.bounds(), state);
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
        let fb = (self.frame.width as f32, self.frame.height as f32);
        let state = tree.state.downcast_mut::<KvmDisplayState>();

        if let Some(message) = interpret_event(state, self.pointer_mode, fb, bounds, cursor, event)
        {
            shell.publish(message);
            shell.capture_event();
        }
    }

    fn draw(
        &self,
        _tree: &Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        renderer.draw_primitive(
            bounds,
            KvmPrimitive {
                tab_id: self.frame.tab_id,
                generation: self.frame.generation,
                width: self.frame.width,
                height: self.frame.height,
                pixels: Arc::clone(&self.frame.pixels),
            },
        );
    }
}

struct KvmPrimitive {
    tab_id: u64,
    generation: u64,
    width: u32,
    height: u32,
    pixels: Arc<[u8]>,
}

impl std::fmt::Debug for KvmPrimitive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KvmPrimitive")
            .field("tab_id", &self.tab_id)
            .field("generation", &self.generation)
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

impl primitive::Primitive for KvmPrimitive {
    type Pipeline = KvmPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &advanced::graphics::Viewport,
    ) {
        let key = (self.tab_id, self.generation);
        if !needs_upload(key, pipeline.key) {
            return;
        }

        if needs_resize((self.width, self.height), pipeline.texture_size) {
            let size = wgpu::Extent3d {
                width: self.width.max(1),
                height: self.height.max(1),
                depth_or_array_layers: 1,
            };
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("kvm-display-texture"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("kvm-display-bind-group"),
                layout: &pipeline.bind_group_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                    },
                ],
            });
            pipeline.texture = Some(texture);
            pipeline.bind_group = Some(bind_group);
            pipeline.texture_size = (self.width, self.height);
        }

        if let Some(texture) = &pipeline.texture {
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &self.pixels,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * self.width.max(1)),
                    rows_per_image: Some(self.height.max(1)),
                },
                wgpu::Extent3d {
                    width: self.width.max(1),
                    height: self.height.max(1),
                    depth_or_array_layers: 1,
                },
            );
        }

        pipeline.key = Some(key);
    }

    fn draw(&self, pipeline: &Self::Pipeline, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        let Some(bind_group) = &pipeline.bind_group else {
            return true;
        };
        render_pass.set_pipeline(&pipeline.render_pipeline);
        render_pass.set_bind_group(0, bind_group, &[]);
        render_pass.set_vertex_buffer(0, pipeline.vertex_buffer.slice(..));
        render_pass.draw(0..4, 0..1);
        true
    }
}

struct KvmPipeline {
    render_pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    vertex_buffer: wgpu::Buffer,
    texture: Option<wgpu::Texture>,
    bind_group: Option<wgpu::BindGroup>,
    texture_size: (u32, u32),
    key: Option<(u64, u64)>,
}

impl primitive::Pipeline for KvmPipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("kvm-display-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("kvm-display-bind-group-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("kvm-display-pipeline-layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("kvm-display-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[VERTEX_BUFFER_LAYOUT],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                ..wgpu::PrimitiveState::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("kvm-display-sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..wgpu::SamplerDescriptor::default()
        });

        let vertex_bytes: Vec<u8> = VERTICES
            .iter()
            .flatten()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("kvm-display-vertices"),
            contents: &vertex_bytes,
            usage: wgpu::BufferUsages::VERTEX,
        });

        Self {
            render_pipeline,
            bind_group_layout,
            sampler,
            vertex_buffer,
            texture: None,
            bind_group: None,
            texture_size: (0, 0),
            key: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key::Named;

    #[test]
    fn letterboxed_size_preserves_aspect_ratio_when_area_is_wider() {
        let (w, h) = letterboxed_size(Size::new(1600.0, 300.0), (800.0, 600.0));
        assert_eq!((w, h), (400.0, 300.0));
    }

    #[test]
    fn letterboxed_size_preserves_aspect_ratio_when_area_is_taller() {
        let (w, h) = letterboxed_size(Size::new(300.0, 1600.0), (800.0, 600.0));
        assert_eq!((w, h), (300.0, 225.0));
    }

    #[test]
    fn map_to_framebuffer_scales_from_bounds_local_point() {
        // A 800x600 fb displayed at exactly 400x300 (the widget's own
        // bounds size) — a point at the centre of the bounds maps to the
        // centre of the framebuffer.
        let bounds_size = Size::new(400.0, 300.0);
        let fb = (800.0, 600.0);

        let top_left = map_to_framebuffer(bounds_size, fb, Point::new(0.0, 0.0));
        assert_eq!(top_left, Point::new(0.0, 0.0));

        let centre = map_to_framebuffer(bounds_size, fb, Point::new(200.0, 150.0));
        assert_eq!(centre, Point::new(400.0, 300.0));
    }

    #[test]
    fn map_to_framebuffer_clamps_past_the_edges() {
        let bounds_size = Size::new(400.0, 300.0);
        let fb = (800.0, 600.0);

        let corner = map_to_framebuffer(bounds_size, fb, Point::new(1000.0, 1000.0));
        assert_eq!(corner, Point::new(fb.0 - 1.0, fb.1 - 1.0));
    }

    #[test]
    fn translate_button_maps_known_buttons() {
        assert_eq!(
            translate_button(mouse::Button::Left),
            Some(PointerButton::Left)
        );
        assert_eq!(
            translate_button(mouse::Button::Right),
            Some(PointerButton::Right)
        );
        assert_eq!(
            translate_button(mouse::Button::Middle),
            Some(PointerButton::Middle)
        );
        assert_eq!(
            translate_button(mouse::Button::Back),
            Some(PointerButton::Back)
        );
        assert_eq!(
            translate_button(mouse::Button::Forward),
            Some(PointerButton::Forward)
        );
        assert_eq!(translate_button(mouse::Button::Other(6)), None);
    }

    fn key_event(pressed: bool, key: keyboard::Key) -> Event {
        let physical_key =
            keyboard::key::Physical::Unidentified(keyboard::key::NativeCode::Unidentified);
        let modifiers = keyboard::Modifiers::default();
        if pressed {
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: key.clone(),
                modified_key: key,
                physical_key,
                location: keyboard::Location::Standard,
                modifiers,
                text: None,
                repeat: false,
            })
        } else {
            Event::Keyboard(keyboard::Event::KeyReleased {
                key: key.clone(),
                modified_key: key,
                physical_key,
                location: keyboard::Location::Standard,
                modifiers,
            })
        }
    }

    const BOUNDS: Rectangle = Rectangle {
        x: 100.0,
        y: 50.0,
        width: 400.0,
        height: 300.0,
    };
    const FB: (f32, f32) = (800.0, 600.0);

    #[test]
    fn left_click_inside_bounds_focuses_and_publishes_pressed_button() {
        let mut state = KvmDisplayState::default();
        let cursor = mouse::Cursor::Available(Point::new(200.0, 150.0)); // inside BOUNDS
        let event = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));

        let message = interpret_event(
            &mut state,
            PointerMode::Absolute,
            FB,
            BOUNDS,
            cursor,
            &event,
        );

        assert!(state.focused);
        match message {
            Some(Message::Button { button, pressed }) => {
                assert_eq!(button, PointerButton::Left);
                assert!(pressed);
            }
            other => panic!("expected a pressed left-button message, got {other:?}"),
        }
    }

    #[test]
    fn left_click_outside_bounds_unfocuses_and_publishes_nothing() {
        let mut state = KvmDisplayState { focused: true };
        let cursor = mouse::Cursor::Available(Point::new(0.0, 0.0)); // outside BOUNDS
        let event = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));

        let message = interpret_event(
            &mut state,
            PointerMode::Absolute,
            FB,
            BOUNDS,
            cursor,
            &event,
        );

        assert!(!state.focused);
        assert!(message.is_none());
    }

    #[test]
    fn button_released_publishes_regardless_of_focus() {
        let mut state = KvmDisplayState::default();
        let cursor = mouse::Cursor::Available(Point::new(0.0, 0.0)); // outside BOUNDS
        let event = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Right));

        let message = interpret_event(
            &mut state,
            PointerMode::Absolute,
            FB,
            BOUNDS,
            cursor,
            &event,
        );

        match message {
            Some(Message::Button { button, pressed }) => {
                assert_eq!(button, PointerButton::Right);
                assert!(!pressed);
            }
            other => panic!("expected a released right-button message, got {other:?}"),
        }
    }

    #[test]
    fn cursor_moved_inside_bounds_publishes_the_mapped_point() {
        let mut state = KvmDisplayState::default();
        // BOUNDS is 400x300 at (100,50) for an 800x600 framebuffer: scale 0.5.
        let cursor = mouse::Cursor::Available(Point::new(300.0, 200.0));
        let event = Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(300.0, 200.0),
        });

        let message = interpret_event(
            &mut state,
            PointerMode::Absolute,
            FB,
            BOUNDS,
            cursor,
            &event,
        );

        match message {
            Some(Message::Pointer(point)) => assert_eq!(point, Point::new(400.0, 300.0)),
            other => panic!("expected a pointer message, got {other:?}"),
        }
    }

    #[test]
    fn cursor_moved_outside_bounds_publishes_nothing() {
        let mut state = KvmDisplayState::default();
        let cursor = mouse::Cursor::Available(Point::new(0.0, 0.0));
        let event = Event::Mouse(mouse::Event::CursorMoved {
            position: Point::new(0.0, 0.0),
        });

        let message = interpret_event(
            &mut state,
            PointerMode::Absolute,
            FB,
            BOUNDS,
            cursor,
            &event,
        );
        assert!(message.is_none());
    }

    #[test]
    fn mouse_events_are_ignored_in_relative_pointer_mode() {
        let mut state = KvmDisplayState::default();
        let cursor = mouse::Cursor::Available(Point::new(200.0, 150.0));
        let event = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));

        let message = interpret_event(
            &mut state,
            PointerMode::Relative,
            FB,
            BOUNDS,
            cursor,
            &event,
        );

        assert!(!state.focused);
        assert!(message.is_none());
    }

    #[test]
    fn key_press_while_unfocused_publishes_nothing() {
        let mut state = KvmDisplayState::default();
        let cursor = mouse::Cursor::Unavailable;
        let event = key_event(true, keyboard::Key::Named(Named::Escape));

        let message = interpret_event(
            &mut state,
            PointerMode::Absolute,
            FB,
            BOUNDS,
            cursor,
            &event,
        );
        assert!(message.is_none());
    }

    #[test]
    fn key_press_and_release_while_focused_publish_key_messages() {
        let mut state = KvmDisplayState { focused: true };
        let cursor = mouse::Cursor::Unavailable;

        let pressed = key_event(true, keyboard::Key::Named(Named::Escape));
        match interpret_event(
            &mut state,
            PointerMode::Absolute,
            FB,
            BOUNDS,
            cursor,
            &pressed,
        ) {
            Some(Message::Key { key, pressed }) => {
                assert_eq!(key, keyboard::Key::Named(Named::Escape));
                assert!(pressed);
            }
            other => panic!("expected a key-pressed message, got {other:?}"),
        }

        let released = key_event(false, keyboard::Key::Named(Named::Escape));
        match interpret_event(
            &mut state,
            PointerMode::Absolute,
            FB,
            BOUNDS,
            cursor,
            &released,
        ) {
            Some(Message::Key { key, pressed }) => {
                assert_eq!(key, keyboard::Key::Named(Named::Escape));
                assert!(!pressed);
            }
            other => panic!("expected a key-released message, got {other:?}"),
        }
    }

    #[test]
    fn needs_upload_when_generation_or_tab_changes() {
        assert!(needs_upload((1, 1), None));
        assert!(needs_upload((1, 2), Some((1, 1))));
        assert!(needs_upload((2, 1), Some((1, 1))));
        assert!(!needs_upload((1, 1), Some((1, 1))));
    }

    #[test]
    fn needs_resize_when_dimensions_change() {
        assert!(needs_resize((800, 600), (0, 0)));
        assert!(needs_resize((1920, 1080), (800, 600)));
        assert!(!needs_resize((800, 600), (800, 600)));
    }

    #[test]
    fn focusable_state_tracks_focus() {
        let mut state = KvmDisplayState::default();
        assert!(!operation::Focusable::is_focused(&state));

        operation::Focusable::focus(&mut state);
        assert!(operation::Focusable::is_focused(&state));

        operation::Focusable::unfocus(&mut state);
        assert!(!operation::Focusable::is_focused(&state));
    }
}
