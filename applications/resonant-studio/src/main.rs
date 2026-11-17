//! Main entry point for Resonant Studio.

mod renderer;

use std::{
    error::Error,
    sync::Arc,
    time::{Duration, Instant},
};

use renderer::Renderer;
use resonant_audio::{
    command::{NoteCommand, ScheduledNoteCommand},
    device::ScheduledAudioCommandOutput,
    envelope::AdsrParameters,
    oscillator::Waveform,
    sample::SampleBank,
};
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
    audio_output: Option<ScheduledAudioCommandOutput>,
    next_preview_frame: u64,
    next_preview_voice_id: u64,
    preview_start_frame: u64,
    preview_end_frame: u64,
}

/// Decodes an embedded mono, 16-bit PCM WAV into the sample bank.
fn add_embedded_sample(bank: &mut SampleBank, name: &str, bytes: &[u8]) -> Result<(), String> {
    let mut reader = hound::WavReader::new(std::io::Cursor::new(bytes))
        .map_err(|error| format!("Could not decode {name} WAV: {error}"))?;

    let spec = reader.spec();

    if spec.channels != 1
        || spec.sample_format != hound::SampleFormat::Int
        || spec.bits_per_sample != 16
    {
        return Err(format!("{name} must be a mono, 16-bit PCM WAV file."));
    }

    let samples = reader
        .samples::<i16>()
        .map(|sample| {
            sample
                .map(|value| f32::from(value) / 32768.0)
                .map_err(|error| format!("Could not read {name} audio: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    if bank.add_sample(spec.sample_rate, samples).is_none() {
        return Err(format!("{name} contains invalid sample data."));
    }

    Ok(())
}

/// Loads the original Browser samples before the audio callback starts.
fn load_browser_sample_bank() -> Result<SampleBank, String> {
    let mut bank = SampleBank::new();

    add_embedded_sample(
        &mut bank,
        "Drum Kit",
        include_bytes!("../assets/samples/drum-kit-kick.wav"),
    )?;

    add_embedded_sample(
        &mut bank,
        "Percussion",
        include_bytes!("../assets/samples/percussion-wood-shaker.wav"),
    )?;

    add_embedded_sample(
        &mut bank,
        "Atmosphere",
        include_bytes!("../assets/samples/atmosphere-evolving-pad.wav"),
    )?;

    Ok(bank)
}

impl ResonantStudio {
    /// Schedules a short audition note for a supported Browser instrument.
    fn schedule_browser_preview(&mut self, asset_id: u32) {
        let Some(output) = self.audio_output.as_mut() else {
            eprintln!("Browser audition unavailable: no audio output stream.");
            return;
        };

        let voice_id = self.next_preview_voice_id;
        self.next_preview_voice_id = self.next_preview_voice_id.saturating_add(1);

        let current_frame = output.current_frame();
        let sample_rate_hz = output.sample_rate_hz();
        let start_frame = current_frame
            .saturating_add(2048)
            .max(self.next_preview_frame);

        let (preview_end_frame, next_preview_frame) = match asset_id {
            0..=2 => {
                let (frequency_hz, gain, waveform) = match asset_id {
                    0 => (440.0, 0.18, Waveform::Sawtooth),
                    1 => (523.25, 0.15, Waveform::Triangle),
                    2 => (82.41, 0.22, Waveform::Sine),
                    _ => unreachable!(),
                };

                let duration_frames = u64::from(sample_rate_hz).saturating_mul(45) / 100;
                let note_off_frame = start_frame.saturating_add(duration_frames);

                let note_on = ScheduledNoteCommand {
                    frame: start_frame,
                    command: NoteCommand::NoteOnWithWaveform {
                        voice_id,
                        frequency_hz,
                        gain,
                        waveform,
                    },
                };

                if let Err(error) = output.try_schedule(note_on) {
                    eprintln!("Could not schedule Browser audition note-on: {error:?}");
                    return;
                }

                let note_off = ScheduledNoteCommand {
                    frame: note_off_frame,
                    command: NoteCommand::NoteOff { voice_id },
                };

                if let Err(error) = output.try_schedule(note_off) {
                    eprintln!("Could not schedule Browser audition note-off: {error:?}");
                    return;
                }

                let release_frames = u64::from(sample_rate_hz).saturating_mul(12) / 100;

                (
                    note_off_frame.saturating_add(release_frames),
                    note_off_frame.saturating_add(1),
                )
            }
            3..=5 => {
                let (sample_id, gain, duration_millis) = match asset_id {
                    3 => (0, 0.85, 620_u64),
                    4 => (1, 0.65, 480_u64),
                    5 => (2, 0.50, 3_200_u64),
                    _ => unreachable!(),
                };

                let duration_frames =
                    u64::from(sample_rate_hz).saturating_mul(duration_millis) / 1_000;
                let preview_end_frame = start_frame.saturating_add(duration_frames);

                let sample_on = ScheduledNoteCommand {
                    frame: start_frame,
                    command: NoteCommand::SampleOn {
                        voice_id,
                        sample_id,
                        gain,
                    },
                };

                if let Err(error) = output.try_schedule(sample_on) {
                    eprintln!("Could not schedule Browser sample audition: {error:?}");
                    return;
                }

                (preview_end_frame, preview_end_frame.saturating_add(1))
            }
            _ => return,
        };

        if self.preview_end_frame <= current_frame {
            self.preview_start_frame = start_frame;
        } else {
            self.preview_start_frame = self.preview_start_frame.min(start_frame);
        }

        self.preview_end_frame = self.preview_end_frame.max(preview_end_frame);
        self.next_preview_frame = next_preview_frame;
    }
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
                if let Some(envelope) = AdsrParameters::new(0.008, 0.08, 0.7, 0.12) {
                    match load_browser_sample_bank() {
                        Ok(sample_bank) => {
                            eprintln!("Loaded {} original Browser samples.", sample_bank.len());

                            match ScheduledAudioCommandOutput::start_with_envelope_and_samples(
                                8,
                                64,
                                envelope,
                                sample_bank,
                            ) {
                                Ok(output) => {
                                    eprintln!(
                                        "Browser audition audio ready: {} Hz.",
                                        output.sample_rate_hz()
                                    );
                                    self.audio_output = Some(output);
                                }
                                Err(error) => {
                                    eprintln!("Browser audition audio unavailable: {error}");
                                }
                            }
                        }
                        Err(error) => {
                            eprintln!("Browser sample loading failed: {error}");
                        }
                    }
                } else {
                    eprintln!("Browser audition audio unavailable: invalid envelope.");
                }

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
                    let preview_request = renderer.take_preview_request();

                    if let Some(asset_id) = preview_request {
                        self.schedule_browser_preview(asset_id);
                    }

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

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let current_audio_frame = self
            .audio_output
            .as_ref()
            .map(ScheduledAudioCommandOutput::current_frame);

        let audition_pending =
            current_audio_frame.is_some_and(|frame| frame < self.preview_end_frame);

        let audition_active = current_audio_frame.is_some_and(|frame| {
            frame >= self.preview_start_frame && frame < self.preview_end_frame
        });

        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        renderer.update_audition(audition_active);

        if renderer.is_playing() {
            renderer.advance_playback();
        }

        if renderer.is_playing() || audition_pending {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }

            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(16),
            ));
        } else {
            event_loop.set_control_flow(ControlFlow::Wait);
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
