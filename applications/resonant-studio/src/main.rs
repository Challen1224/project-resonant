//! Main entry point for Resonant Studio.

mod renderer;

use std::{error::Error, sync::Arc};

use renderer::Renderer;
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalPosition},
    event::{ElementState, MouseButton, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Theme, Window, WindowId},
};

/// Owns the initial Resonant Studio application window and renderer.
#[derive(Default)]
struct ResonantStudio {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    cursor_position: Option<PhysicalPosition<f64>>,
}

impl ApplicationHandler for ResonantStudio {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attributes = Window::default_attributes()
            .with_title("Resonant Studio — Music without boundaries")
            .with_inner_size(LogicalSize::new(1440.0, 900.0))
            .with_min_inner_size(LogicalSize::new(1100.0, 700.0))
            .with_theme(Some(Theme::Dark));

        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => {
                eprintln!("Failed to create Resonant Studio window: {error}");
                event_loop.exit();
                return;
            }
        };

        match pollster::block_on(Renderer::new(window.clone())) {
            Ok(renderer) => {
                self.renderer = Some(renderer);
                self.window = Some(window.clone());
                window.request_redraw();
            }
            Err(error) => {
                eprintln!("Failed to initialize Resonant Studio graphics: {error}");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .window
            .as_ref()
            .is_some_and(|window| window.id() != window_id)
        {
            return;
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                if let Some(renderer) = self.renderer.as_mut() {
                    renderer.resize(size);
                }

                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                self.cursor_position = Some(position);
            }

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                if let (Some(renderer), Some(position)) =
                    (self.renderer.as_mut(), self.cursor_position)
                {
                    renderer.handle_pointer_press(position);

                    if let Some(window) = self.window.as_ref() {
                        window.request_redraw();
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                if let Some(renderer) = self.renderer.as_mut() {
                    if let Err(error) = renderer.render() {
                        eprintln!("GPU frame presentation failed: {error}");

                        if matches!(error, wgpu::SurfaceError::OutOfMemory) {
                            event_loop.exit();
                        }
                    }
                }
            }

            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut application = ResonantStudio::default();
    event_loop.run_app(&mut application)?;

    Ok(())
}
