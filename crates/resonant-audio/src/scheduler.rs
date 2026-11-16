//! Absolute-frame scheduling for real-time audio rendering.

use crate::command::{NoteCommand, ScheduledNoteCommand, ScheduledNoteCommandReceiver};
use crate::voice_manager::AudioVoiceManager;

/// Maximum number of scheduled commands applied during one output frame.
///
/// This bounds command-processing work in the audio rendering path.
const MAX_COMMANDS_PER_FRAME: usize = 256;

/// Renders audio while applying note commands at their scheduled frame.
///
/// The renderer retains future commands between calls and maintains one
/// absolute output-frame counter across any number of rendering buffers.
///
/// Commands whose scheduled frame has already passed are applied at the next
/// rendered frame. At most 256 due commands are applied per frame; additional
/// commands remain pending for subsequent frames.
pub struct ScheduledNoteRenderer {
    receiver: ScheduledNoteCommandReceiver,
    voice_manager: AudioVoiceManager,
    next_frame: u64,
    pending_command: Option<ScheduledNoteCommand>,
}

impl ScheduledNoteRenderer {
    /// Creates a scheduled renderer starting at the supplied absolute frame.
    #[must_use]
    pub fn new(
        receiver: ScheduledNoteCommandReceiver,
        voice_manager: AudioVoiceManager,
        start_frame: u64,
    ) -> Self {
        Self {
            receiver,
            voice_manager,
            next_frame: start_frame,
            pending_command: None,
        }
    }

    /// Returns the absolute frame that will be rendered next.
    #[must_use]
    pub fn next_frame(&self) -> u64 {
        self.next_frame
    }

    /// Returns the number of currently allocated voices.
    #[must_use]
    pub fn voice_count(&self) -> usize {
        self.voice_manager.voice_count()
    }

    /// Renders one sample and advances the absolute output-frame position.
    ///
    /// Due commands are applied before the sample for that frame is generated.
    #[must_use]
    pub fn next_sample(&mut self) -> f32 {
        self.apply_due_commands();

        let sample = self.voice_manager.next_sample();
        self.next_frame = self.next_frame.saturating_add(1);

        sample
    }

    /// Renders consecutive mono output frames into a caller-provided buffer.
    ///
    /// The buffer is reused directly; this method does not allocate.
    pub fn render_mono(&mut self, output: &mut [f32]) {
        for sample in output {
            *sample = self.next_sample();
        }
    }

    fn apply_due_commands(&mut self) {
        for _ in 0..MAX_COMMANDS_PER_FRAME {
            if self.pending_command.is_none() {
                self.pending_command = self.receiver.try_receive();
            }

            let Some(scheduled) = self.pending_command else {
                break;
            };

            if scheduled.frame > self.next_frame {
                break;
            }

            self.pending_command = None;
            self.apply_command(scheduled.command);
        }
    }

    fn apply_command(&mut self, command: NoteCommand) {
        match command {
            NoteCommand::NoteOn {
                voice_id,
                frequency_hz,
                gain,
            } => {
                let _ = self.voice_manager.note_on(voice_id, frequency_hz, gain);
            }
            NoteCommand::NoteOnWithWaveform {
                voice_id,
                frequency_hz,
                gain,
                waveform,
            } => {
                let _ = self.voice_manager.note_on_with_waveform(
                    voice_id,
                    frequency_hz,
                    gain,
                    waveform,
                );
            }
            NoteCommand::NoteOff { voice_id } => {
                let _ = self.voice_manager.note_off(voice_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ScheduledNoteRenderer;
    use crate::command::{NoteCommand, ScheduledNoteCommand, scheduled_note_command_channel};
    use crate::voice_manager::AudioVoiceManager;

    fn renderer(
        start_frame: u64,
    ) -> (
        crate::command::ScheduledNoteCommandSender,
        ScheduledNoteRenderer,
    ) {
        let (sender, receiver) = scheduled_note_command_channel(16).expect("valid queue capacity");
        let manager = AudioVoiceManager::new(4, 4.0).expect("valid voice manager configuration");

        (
            sender,
            ScheduledNoteRenderer::new(receiver, manager, start_frame),
        )
    }

    #[test]
    fn note_starts_at_its_exact_scheduled_frame() {
        let (mut sender, mut renderer) = renderer(0);

        let command = ScheduledNoteCommand {
            frame: 2,
            command: NoteCommand::NoteOn {
                voice_id: 1,
                frequency_hz: 1.0,
                gain: 0.5,
            },
        };

        assert_eq!(sender.try_send(command), Ok(()));

        assert_eq!(renderer.next_sample(), 0.0);
        assert_eq!(renderer.next_sample(), 0.0);
        assert_eq!(renderer.voice_count(), 0);

        assert_eq!(renderer.next_sample(), 0.0);
        assert_eq!(renderer.voice_count(), 1);

        assert!((renderer.next_sample() - 0.5).abs() < 0.0001);
        assert_eq!(renderer.next_frame(), 4);
    }

    #[test]
    fn future_command_is_preserved_across_render_buffers() {
        let (mut sender, mut renderer) = renderer(0);

        let command = ScheduledNoteCommand {
            frame: 2,
            command: NoteCommand::NoteOn {
                voice_id: 1,
                frequency_hz: 1.0,
                gain: 0.5,
            },
        };

        assert_eq!(sender.try_send(command), Ok(()));

        let mut first_buffer = [0.0; 2];
        renderer.render_mono(&mut first_buffer);

        assert_eq!(first_buffer, [0.0, 0.0]);
        assert_eq!(renderer.next_frame(), 2);
        assert_eq!(renderer.voice_count(), 0);

        let mut second_buffer = [0.0; 2];
        renderer.render_mono(&mut second_buffer);

        assert_eq!(second_buffer[0], 0.0);
        assert!((second_buffer[1] - 0.5).abs() < 0.0001);
        assert_eq!(renderer.next_frame(), 4);
        assert_eq!(renderer.voice_count(), 1);
    }

    #[test]
    fn commands_at_the_same_frame_are_applied_in_fifo_order() {
        let (mut sender, mut renderer) = renderer(0);

        let start = ScheduledNoteCommand {
            frame: 0,
            command: NoteCommand::NoteOn {
                voice_id: 7,
                frequency_hz: 1.0,
                gain: 0.5,
            },
        };
        let stop = ScheduledNoteCommand {
            frame: 0,
            command: NoteCommand::NoteOff { voice_id: 7 },
        };

        assert_eq!(sender.try_send(start), Ok(()));
        assert_eq!(sender.try_send(stop), Ok(()));

        let _ = renderer.next_sample();

        assert_eq!(renderer.voice_count(), 0);
    }

    #[test]
    fn late_commands_are_applied_at_the_next_available_frame() {
        let (mut sender, mut renderer) = renderer(10);

        let command = ScheduledNoteCommand {
            frame: 5,
            command: NoteCommand::NoteOn {
                voice_id: 1,
                frequency_hz: 1.0,
                gain: 0.5,
            },
        };

        assert_eq!(sender.try_send(command), Ok(()));

        let _ = renderer.next_sample();

        assert_eq!(renderer.voice_count(), 1);
        assert_eq!(renderer.next_frame(), 11);
    }

    #[test]
    fn renderer_preserves_absolute_start_frame() {
        let (_, mut renderer) = renderer(1_000);

        assert_eq!(renderer.next_frame(), 1_000);

        let _ = renderer.next_sample();

        assert_eq!(renderer.next_frame(), 1_001);
    }
}
