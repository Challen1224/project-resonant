//! GPU surface initialization and frame presentation.

use std::{error::Error, io, sync::Arc, time::Instant};

use wgpu_text::{
    BrushBuilder, TextBrush,
    glyph_brush::{Section, Text, ab_glyph::FontArc},
};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    window::Window,
};

/// Owns the GPU resources required to present Resonant Studio frames.
pub(crate) struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    configuration: wgpu::SurfaceConfiguration,
    viewport_buffer: wgpu::Buffer,
    viewport_bind_group: wgpu::BindGroup,
    ui_pipeline: wgpu::RenderPipeline,
    text_brush: TextBrush,
    selected_clip: f32,
    selected_browser_asset: f32,
    playhead_x: f32,
    is_playing: bool,
    last_playback_update: Instant,
}

impl Renderer {
    /// Creates a GPU device and configures a surface for the application window.
    // GPU initialization is intentionally kept sequential because each resource
    // depends on the resource created immediately before it.
    #[allow(clippy::too_many_lines)]
    pub(crate) async fn new(window: Arc<Window>) -> Result<Self, Box<dyn Error>> {
        let instance = wgpu::Instance::default();

        let surface = instance.create_surface(window.clone())?;

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .map_err(|error| io::Error::other(format!("No compatible GPU adapter: {error}")))?;

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Resonant Studio GPU Device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                ..Default::default()
            })
            .await
            .map_err(|error| io::Error::other(format!("GPU device request failed: {error}")))?;

        let size = window.inner_size();
        let capabilities = surface.get_capabilities(&adapter);

        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| io::Error::other("GPU surface exposes no supported formats"))?;

        let present_mode = capabilities
            .present_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::PresentMode::Fifo)
            .or_else(|| capabilities.present_modes.first().copied())
            .ok_or_else(|| io::Error::other("GPU surface exposes no presentation modes"))?;

        let alpha_mode = capabilities
            .alpha_modes
            .first()
            .copied()
            .ok_or_else(|| io::Error::other("GPU surface exposes no alpha modes"))?;

        let configuration = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &configuration);

        let viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Resonant Studio Viewport Interaction State"),
            size: 24,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let viewport_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Resonant Studio Viewport Bind Group Layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let viewport_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Resonant Studio Viewport Bind Group"),
            layout: &viewport_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: viewport_buffer.as_entire_binding(),
            }],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Resonant Studio Interface Shader"),
            source: wgpu::ShaderSource::Wgsl(UI_SHADER.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Resonant Studio Interface Pipeline Layout"),
            bind_group_layouts: &[&viewport_bind_group_layout],
            push_constant_ranges: &[],
        });

        let ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Resonant Studio Interface Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let font = FontArc::try_from_slice(include_bytes!("../assets/fonts/Inter.ttf"))?;

        let text_brush = BrushBuilder::using_font(font).build(
            &device,
            configuration.width,
            configuration.height,
            format,
        );

        Ok(Self {
            surface,
            device,
            queue,
            configuration,
            viewport_buffer,
            viewport_bind_group,
            ui_pipeline,
            text_brush,
            selected_clip: -1.0,
            selected_browser_asset: -1.0,
            playhead_x: 0.0,
            is_playing: false,
            last_playback_update: Instant::now(),
        })
    }

    /// Updates the surface dimensions after a window resize.
    pub(crate) fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }

        self.configuration.width = size.width;
        self.configuration.height = size.height;

        self.surface.configure(&self.device, &self.configuration);

        self.text_brush.resize_view(
            viewport_dimension(size.width),
            viewport_dimension(size.height),
            &self.queue,
        );
    }

    /// Updates the playhead and clip selection from a timeline click.
    pub(crate) fn handle_pointer_press(&mut self, position: PhysicalPosition<f64>) {
        let x = pointer_dimension(position.x);
        let y = pointer_dimension(position.y);

        let timeline_origin_x = 420.0;
        let arrangement_header = 104.0;
        let mixer_top = viewport_dimension(self.configuration.height) - 220.0;

        // Toolbar transport controls.
        if y < 58.0 {
            if (292.0..354.0).contains(&x) {
                self.is_playing = true;
                self.last_playback_update = Instant::now();
            } else if (362.0..424.0).contains(&x) {
                self.is_playing = false;
                self.playhead_x = 0.0;
                self.last_playback_update = Instant::now();
            }
            return;
        }

        // Browser asset selection.
        if x < 260.0 && y < mixer_top {
            self.selected_browser_asset = -1.0;

            if (12.0..248.0).contains(&x) {
                for (asset_id, row_start) in [
                    (0.0, 154.0),
                    (1.0, 188.0),
                    (2.0, 222.0),
                    (3.0, 316.0),
                    (4.0, 350.0),
                    (5.0, 384.0),
                ] {
                    if (row_start..row_start + 36.0).contains(&y) {
                        self.selected_browser_asset = asset_id;
                        break;
                    }
                }
            }

            return;
        }

        if x < timeline_origin_x || y >= mixer_top {
            return;
        }

        let timeline_x = x - timeline_origin_x;
        self.playhead_x = timeline_x;
        self.selected_clip = -1.0;

        // The ruler moves the playhead without selecting a clip.
        if y < arrangement_header {
            return;
        }

        let lane_y = y - arrangement_header;

        // Drums: five separate clips.
        if (18.0..114.0).contains(&lane_y) {
            for (clip_id, start) in [
                (0.0, 28.0),
                (1.0, 188.0),
                (2.0, 348.0),
                (3.0, 508.0),
                (4.0, 668.0),
            ] {
                if (start..start + 120.0).contains(&timeline_x) {
                    self.selected_clip = clip_id;
                    return;
                }
            }
        }

        // Bass.
        if (162.0..258.0).contains(&lane_y) && (68.0..308.0).contains(&timeline_x) {
            self.selected_clip = 5.0;
            return;
        }

        // Lead.
        if (306.0..402.0).contains(&lane_y) && (228.0..548.0).contains(&timeline_x) {
            self.selected_clip = 6.0;
            return;
        }

        // Atmos.
        if (450.0..546.0).contains(&lane_y) && (28.0..468.0).contains(&timeline_x) {
            self.selected_clip = 7.0;
        }
    }

    /// Returns whether the transport is currently playing.
    pub(crate) fn is_playing(&self) -> bool {
        self.is_playing
    }

    /// Advances the playhead using elapsed wall-clock time.
    pub(crate) fn advance_playback(&mut self) {
        if !self.is_playing {
            return;
        }

        let now = Instant::now();
        let elapsed = now.duration_since(self.last_playback_update).as_secs_f32();
        self.last_playback_update = now;

        // Prototype tempo: 80 timeline pixels per second.
        self.playhead_x += elapsed * 80.0;

        let timeline_width = viewport_dimension(self.configuration.width) - 420.0;
        if self.playhead_x >= timeline_width {
            self.playhead_x = timeline_width.max(0.0);
            self.is_playing = false;
        }
    }

    /// Clears and presents one GPU-rendered frame.
    pub(crate) fn render(&mut self) -> Result<(), wgpu::SurfaceError> {
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.configuration);
                return Ok(());
            }
            Err(wgpu::SurfaceError::Timeout) => return Ok(()),
            Err(error) => return Err(error),
        };

        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut viewport_bytes = [0_u8; 24];
        viewport_bytes[0..4]
            .copy_from_slice(&viewport_dimension(self.configuration.width).to_ne_bytes());
        viewport_bytes[4..8]
            .copy_from_slice(&viewport_dimension(self.configuration.height).to_ne_bytes());
        viewport_bytes[8..12].copy_from_slice(&self.selected_clip.to_ne_bytes());
        viewport_bytes[12..16].copy_from_slice(&self.playhead_x.to_ne_bytes());
        let playing = f32::from(self.is_playing);
        viewport_bytes[16..20].copy_from_slice(&playing.to_ne_bytes());
        viewport_bytes[20..24].copy_from_slice(&self.selected_browser_asset.to_ne_bytes());

        self.queue
            .write_buffer(&self.viewport_buffer, 0, &viewport_bytes);

        let width = viewport_dimension(self.configuration.width);
        let height = viewport_dimension(self.configuration.height);
        let text_sections = build_text_sections(width, height);

        if let Err(error) = self
            .text_brush
            .queue(&self.device, &self.queue, text_sections)
        {
            eprintln!("Interface text rendering failed: {error}");
        }

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Resonant Studio Frame Encoder"),
            });

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Resonant Studio Background Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.035,
                            g: 0.04,
                            b: 0.055,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self.ui_pipeline);
            render_pass.set_bind_group(0, &self.viewport_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
            self.text_brush.draw(&mut render_pass);
        }

        self.queue.submit(Some(encoder.finish()));
        frame.present();

        Ok(())
    }
}

/// Creates one styled text section for the Studio interface.
fn make_text_section(
    text: &'static str,
    position: (f32, f32),
    bounds: (f32, f32),
    scale: f32,
    color: [f32; 4],
) -> Section<'static> {
    Section::default()
        .with_screen_position(position)
        .with_bounds(bounds)
        .add_text(Text::new(text).with_scale(scale).with_color(color))
}

/// Creates the Resonant Studio brand label.
fn make_brand_text_section(width: f32) -> Section<'static> {
    make_text_section(
        "RESONANT",
        (20.0, 18.0),
        (width - 40.0, 40.0),
        32.0,
        [0.91, 0.91, 0.97, 1.0],
    )
}

/// Creates a prominent transport control label.
fn make_transport_text_section(
    text: &'static str,
    position: (f32, f32),
    color: [f32; 4],
) -> Section<'static> {
    make_text_section(text, position, (50.0, 26.0), 16.0, color)
}

/// Builds the text labels displayed across the Studio interface.
#[allow(clippy::too_many_lines)]
fn build_text_sections(width: f32, height: f32) -> [Section<'static>; 24] {
    [
        make_brand_text_section(width),
        make_transport_text_section("PLAY", (307.0, 17.0), [1.0, 1.0, 1.0, 1.0]),
        make_transport_text_section("STOP", (379.0, 17.0), [0.91, 0.90, 0.96, 1.0]),
        make_text_section(
            "BROWSER",
            (20.0, 78.0),
            (220.0, 28.0),
            26.0,
            [0.88, 0.89, 0.96, 1.0],
        ),
        make_text_section(
            "INSTRUMENTS",
            (20.0, 130.0),
            (220.0, 22.0),
            17.0,
            [0.66, 0.69, 0.80, 1.0],
        ),
        make_text_section(
            "Analog Synth",
            (30.0, 164.0),
            (210.0, 24.0),
            20.0,
            [0.91, 0.88, 1.0, 1.0],
        ),
        make_text_section(
            "Electric Piano",
            (30.0, 198.0),
            (210.0, 24.0),
            20.0,
            [0.84, 0.88, 0.96, 1.0],
        ),
        make_text_section(
            "Sub Bass",
            (30.0, 232.0),
            (210.0, 24.0),
            20.0,
            [0.84, 0.88, 0.96, 1.0],
        ),
        make_text_section(
            "SAMPLES",
            (20.0, 292.0),
            (220.0, 22.0),
            17.0,
            [0.66, 0.69, 0.80, 1.0],
        ),
        make_text_section(
            "Drum Kit",
            (30.0, 326.0),
            (210.0, 24.0),
            20.0,
            [0.84, 0.91, 0.96, 1.0],
        ),
        make_text_section(
            "Percussion",
            (30.0, 360.0),
            (210.0, 24.0),
            20.0,
            [0.84, 0.88, 0.96, 1.0],
        ),
        make_text_section(
            "Atmosphere",
            (30.0, 394.0),
            (210.0, 24.0),
            20.0,
            [0.84, 0.88, 0.96, 1.0],
        ),
        make_text_section(
            "ARRANGE",
            (284.0, 78.0),
            (width - 300.0, 28.0),
            24.0,
            [0.88, 0.89, 0.96, 1.0],
        ),
        make_text_section(
            "MIXER",
            (20.0, height - 194.0),
            (width - 40.0, 28.0),
            24.0,
            [0.88, 0.89, 0.96, 1.0],
        ),
        make_text_section(
            "DRUMS",
            (280.0, 138.0),
            (120.0, 24.0),
            18.0,
            [0.91, 0.92, 0.98, 1.0],
        ),
        make_text_section(
            "BASS",
            (280.0, 282.0),
            (120.0, 24.0),
            18.0,
            [0.91, 0.92, 0.98, 1.0],
        ),
        make_text_section(
            "LEAD",
            (280.0, 426.0),
            (120.0, 24.0),
            18.0,
            [0.91, 0.92, 0.98, 1.0],
        ),
        make_text_section(
            "ATMOS",
            (280.0, 570.0),
            (120.0, 24.0),
            18.0,
            [0.91, 0.92, 0.98, 1.0],
        ),
        make_text_section(
            "1",
            (430.0, 78.0),
            (32.0, 20.0),
            15.0,
            [0.88, 0.89, 0.96, 1.0],
        ),
        make_text_section(
            "2",
            (590.0, 78.0),
            (32.0, 20.0),
            15.0,
            [0.88, 0.89, 0.96, 1.0],
        ),
        make_text_section(
            "3",
            (750.0, 78.0),
            (32.0, 20.0),
            15.0,
            [0.88, 0.89, 0.96, 1.0],
        ),
        make_text_section(
            "4",
            (910.0, 78.0),
            (32.0, 20.0),
            15.0,
            [0.88, 0.89, 0.96, 1.0],
        ),
        make_text_section(
            "5",
            (1070.0, 78.0),
            (32.0, 20.0),
            15.0,
            [0.88, 0.89, 0.96, 1.0],
        ),
        make_text_section(
            "6",
            (1230.0, 78.0),
            (32.0, 20.0),
            15.0,
            [0.88, 0.89, 0.96, 1.0],
        ),
    ]
}

/// Converts a physical pointer coordinate to the renderer's pixel units.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn pointer_dimension(value: f64) -> f32 {
    value as f32
}

/// Converts a physical surface dimension for the shader's viewport uniform.
/// GPU surface dimensions are bounded by device texture limits and fit in `f32`.
#[allow(clippy::cast_precision_loss)]
fn viewport_dimension(value: u32) -> f32 {
    value as f32
}

/// Draws the initial four-region Resonant Studio interface.
const UI_SHADER: &str = r"
struct Viewport {
    size: vec2<f32>,
    selected_clip: f32,
    playhead_x: f32,
    is_playing: f32,
    selected_browser_asset: f32,
};

@group(0) @binding(0)
var<uniform> viewport: Viewport;

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0)
    );

    return vec4<f32>(positions[index], 0.0, 1.0);
}

fn rounded_box(
    point: vec2<f32>,
    center: vec2<f32>,
    half_size: vec2<f32>,
    radius: f32
) -> f32 {
    let q = abs(point - center) - half_size + vec2<f32>(radius);
    return length(max(q, vec2<f32>(0.0)))
        + min(max(q.x, q.y), 0.0) - radius;
}

@fragment
fn fs_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let x = position.x;
    let y = position.y;
    let width = viewport.size.x;
    let height = viewport.size.y;

    let toolbar_height = 58.0;
    let browser_width = 260.0;
    let mixer_height = 220.0;
    let arrangement_header = 104.0;
    let track_header_width = 160.0;

    let toolbar = vec3<f32>(0.075, 0.080, 0.105);
    let browser = vec3<f32>(0.065, 0.072, 0.095);
    let arrangement = vec3<f32>(0.045, 0.050, 0.070);
    let mixer = vec3<f32>(0.060, 0.067, 0.088);
    let divider = vec3<f32>(0.105, 0.115, 0.145);
    let accent = vec3<f32>(0.36, 0.23, 0.85);
    let panel_header = vec3<f32>(0.085, 0.092, 0.125);
    let panel_header_edge = vec3<f32>(0.16, 0.17, 0.22);

    if y < toolbar_height {
        let point = vec2<f32>(x, y);

        // Play button: brighter violet while playback is active.
        let play_outer = rounded_box(
            point, vec2<f32>(323.0, 29.0),
            vec2<f32>(32.0, 21.0), 9.0
        );
        if play_outer <= 0.0 {
            let play_inner = rounded_box(
                point, vec2<f32>(323.0, 29.0),
                vec2<f32>(30.0, 19.0), 7.0
            );

            if play_inner <= 0.0 {
                if viewport.is_playing > 0.5 {
                    return vec4<f32>(0.46, 0.31, 0.98, 1.0);
                }
                return vec4<f32>(0.32, 0.22, 0.72, 1.0);
            }

            return vec4<f32>(0.19, 0.15, 0.34, 1.0);
        }

        // Stop button: contrasting charcoal surface.
        let stop_outer = rounded_box(
            point, vec2<f32>(393.0, 29.0),
            vec2<f32>(32.0, 21.0), 9.0
        );
        if stop_outer <= 0.0 {
            let stop_inner = rounded_box(
                point, vec2<f32>(393.0, 29.0),
                vec2<f32>(30.0, 19.0), 7.0
            );

            if stop_inner <= 0.0 {
                return vec4<f32>(0.15, 0.16, 0.21, 1.0);
            }

            return vec4<f32>(0.23, 0.24, 0.31, 1.0);
        }

        return vec4<f32>(toolbar, 1.0);
    }

    if y >= height - mixer_height {
        let mixer_header_bottom = height - mixer_height + 52.0;
        if y < height - mixer_height + 2.0 {
            return vec4<f32>(divider, 1.0);
        }
        if y < mixer_header_bottom - 1.0 {
            return vec4<f32>(panel_header, 1.0);
        }
        if y < mixer_header_bottom + 1.0 {
            return vec4<f32>(panel_header_edge, 1.0);
        }
        return vec4<f32>(mixer, 1.0);
    }

    if x < browser_width {
        if x >= browser_width - 2.0 {
            return vec4<f32>(divider, 1.0);
        }
        if y < 110.0 {
            if y >= 108.0 {
                return vec4<f32>(panel_header_edge, 1.0);
            }
            return vec4<f32>(panel_header, 1.0);
        }

        // Highlight the selected Browser asset row.
        var browser_asset_index = -1.0;
        if y >= 154.0 && y < 190.0 {
            browser_asset_index = 0.0;
        } else if y >= 188.0 && y < 224.0 {
            browser_asset_index = 1.0;
        } else if y >= 222.0 && y < 258.0 {
            browser_asset_index = 2.0;
        } else if y >= 316.0 && y < 352.0 {
            browser_asset_index = 3.0;
        } else if y >= 350.0 && y < 386.0 {
            browser_asset_index = 4.0;
        } else if y >= 384.0 && y < 420.0 {
            browser_asset_index = 5.0;
        }

        if browser_asset_index >= 0.0
            && browser_asset_index == viewport.selected_browser_asset
            && x >= 12.0 && x < 248.0
        {
            if x < 16.0 {
                return vec4<f32>(0.52, 0.35, 0.92, 1.0);
            }
            return vec4<f32>(0.115, 0.090, 0.175, 1.0);
        }

        return vec4<f32>(browser, 1.0);
    }

    if y < toolbar_height + 2.0 {
        return vec4<f32>(accent, 1.0);
    }

    if y < arrangement_header {
        if x < browser_width + track_header_width {
            return vec4<f32>(vec3<f32>(0.055, 0.060, 0.082), 1.0);
        }

        let ruler_x = x - browser_width - track_header_width;

        if abs(ruler_x - viewport.playhead_x) < 1.2 {
            return vec4<f32>(0.20, 0.88, 0.92, 1.0);
        }

        let beat_line = abs(fract(ruler_x / 40.0));
        let bar_line = abs(fract(ruler_x / 160.0));

        if bar_line < 0.025 {
            return vec4<f32>(vec3<f32>(0.22, 0.23, 0.30), 1.0);
        }
        if beat_line < 0.025 {
            return vec4<f32>(vec3<f32>(0.12, 0.13, 0.18), 1.0);
        }

        return vec4<f32>(vec3<f32>(0.070, 0.076, 0.100), 1.0);
    }

    if x < browser_width + track_header_width {
        let lane_y = y - arrangement_header;
        let lane_line = abs(fract(lane_y / 144.0));

        if lane_line < 0.012 {
            return vec4<f32>(divider, 1.0);
        }

        return vec4<f32>(vec3<f32>(0.055, 0.060, 0.082), 1.0);
    }

    let timeline_x = x - browser_width - track_header_width;
    let lane_y = y - arrangement_header;

    var color = arrangement;

    // Four alternating track backgrounds.
    let lane_index = floor(lane_y / 144.0);
    if lane_index == 1.0 || lane_index == 3.0 {
        color = vec3<f32>(0.050, 0.056, 0.078);
    }

    // Stable IDs let the renderer highlight the selected clip.
    var clip_index = -1.0;

    if lane_y >= 18.0 && lane_y < 114.0 {
        if timeline_x >= 28.0 && timeline_x < 148.0 {
            clip_index = 0.0;
        } else if timeline_x >= 188.0 && timeline_x < 308.0 {
            clip_index = 1.0;
        } else if timeline_x >= 348.0 && timeline_x < 468.0 {
            clip_index = 2.0;
        } else if timeline_x >= 508.0 && timeline_x < 628.0 {
            clip_index = 3.0;
        } else if timeline_x >= 668.0 && timeline_x < 788.0 {
            clip_index = 4.0;
        }
    } else if lane_y >= 162.0 && lane_y < 258.0
        && timeline_x >= 68.0 && timeline_x < 308.0
    {
        clip_index = 5.0;
    } else if lane_y >= 306.0 && lane_y < 402.0
        && timeline_x >= 228.0 && timeline_x < 548.0
    {
        clip_index = 6.0;
    } else if lane_y >= 450.0 && lane_y < 546.0
        && timeline_x >= 28.0 && timeline_x < 468.0
    {
        clip_index = 7.0;
    }

    // Colored arrangement clips.
    if lane_y >= 18.0 && lane_y < 114.0 {
        if (timeline_x >= 28.0 && timeline_x < 148.0) ||
           (timeline_x >= 188.0 && timeline_x < 308.0) ||
           (timeline_x >= 348.0 && timeline_x < 468.0) ||
           (timeline_x >= 508.0 && timeline_x < 628.0) ||
           (timeline_x >= 668.0 && timeline_x < 788.0) {
            color = vec3<f32>(0.34, 0.20, 0.66);
            if lane_y < 24.0 {
                color = vec3<f32>(0.56, 0.36, 0.90);
            }
        }
    }

    if lane_y >= 162.0 && lane_y < 258.0 {
        if timeline_x >= 68.0 && timeline_x < 308.0 {
            color = vec3<f32>(0.12, 0.40, 0.48);
            if lane_y < 168.0 {
                color = vec3<f32>(0.24, 0.68, 0.75);
            }
        }
    }

    if lane_y >= 306.0 && lane_y < 402.0 {
        if timeline_x >= 228.0 && timeline_x < 548.0 {
            color = vec3<f32>(0.40, 0.25, 0.58);
            if lane_y < 312.0 {
                color = vec3<f32>(0.65, 0.43, 0.82);
            }
        }
    }

    if lane_y >= 450.0 && lane_y < 546.0 {
        if timeline_x >= 28.0 && timeline_x < 468.0 {
            color = vec3<f32>(0.42, 0.25, 0.20);
            if lane_y < 456.0 {
                color = vec3<f32>(0.75, 0.43, 0.30);
            }
        }
    }

    // Highlight the currently selected clip.
    if clip_index >= 0.0 && clip_index == viewport.selected_clip {
        color = mix(color, vec3<f32>(0.92, 0.82, 1.0), 0.48);
    }

    // Beat grid, drawn over the track backgrounds.
    let beat_line = abs(fract(timeline_x / 40.0));
    let bar_line = abs(fract(timeline_x / 160.0));

    if bar_line < 0.018 {
        color = mix(color, vec3<f32>(0.30, 0.31, 0.40), 0.55);
    } else if beat_line < 0.018 {
        color = mix(color, vec3<f32>(0.18, 0.19, 0.25), 0.55);
    }

    // Horizontal track separators.
    if abs(fract(lane_y / 144.0)) < 0.010 {
        color = divider;
    }

    // Draw the playhead above the clips and grid.
    if abs(timeline_x - viewport.playhead_x) < 1.2 {
        color = vec3<f32>(0.20, 0.88, 0.92);
    }

    return vec4<f32>(color, 1.0);
}
";
