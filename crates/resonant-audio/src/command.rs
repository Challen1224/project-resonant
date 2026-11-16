//! Nonblocking single-producer/single-consumer audio commands.

use rtrb::{Consumer, Producer, PushError, RingBuffer};

/// A command sent from the application or sequencer to the audio thread.
///
/// Command values are intentionally small and contain no heap-owned data.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NoteCommand {
    /// Starts a voice identified by `voice_id`.
    NoteOn {
        /// Identifier used to match this voice with its Note Off command.
        voice_id: u64,
        /// Oscillator frequency in hertz.
        frequency_hz: f32,
        /// Linear voice gain in the range 0.0 to 1.0.
        gain: f32,
    },
    /// Starts a voice with an explicitly selected oscillator waveform.
    NoteOnWithWaveform {
        /// Identifier used to match this voice with its Note Off command.
        voice_id: u64,
        /// Oscillator frequency in hertz.
        frequency_hz: f32,
        /// Linear voice gain in the range 0.0 to 1.0.
        gain: f32,
        /// Waveform used by the voice oscillator.
        waveform: crate::oscillator::Waveform,
    },
    /// Begins the release stage for the voice identified by `voice_id`.
    NoteOff {
        /// Identifier of the voice to stop.
        voice_id: u64,
    },
}

/// The sending endpoint of a single-producer/single-consumer command queue.
///
/// Keep this endpoint on exactly one producer thread. It is deliberately not
/// cloneable, and sending requires mutable access.
pub struct NoteCommandSender {
    producer: Producer<NoteCommand>,
}

/// The receiving endpoint of a single-producer/single-consumer command queue.
///
/// Keep this endpoint on exactly one consumer thread, normally the audio
/// callback. It is deliberately not cloneable.
pub struct NoteCommandReceiver {
    consumer: Consumer<NoteCommand>,
}

/// Creates a bounded command queue with the requested capacity.
///
/// The returned sender and receiver are the queue's unique producer and
/// consumer endpoints.
///
/// # Errors
///
/// Returns an error when `capacity` is zero.
pub fn note_command_channel(
    capacity: usize,
) -> Result<(NoteCommandSender, NoteCommandReceiver), &'static str> {
    if capacity == 0 {
        return Err("Note command queue capacity must be greater than zero.");
    }

    let (producer, consumer) = RingBuffer::new(capacity);

    Ok((
        NoteCommandSender { producer },
        NoteCommandReceiver { consumer },
    ))
}

impl NoteCommandSender {
    /// Attempts to enqueue a command without waiting.
    ///
    /// Returns the original command if the queue is full.
    ///
    /// # Errors
    ///
    /// Returns the command unchanged when the queue has no available capacity.
    pub fn try_send(&mut self, command: NoteCommand) -> Result<(), NoteCommand> {
        match self.producer.push(command) {
            Ok(()) => Ok(()),
            Err(PushError::Full(command)) => Err(command),
        }
    }
}

impl NoteCommandReceiver {
    /// Attempts to receive one command without waiting.
    ///
    /// Returns `None` when the queue is empty.
    #[must_use]
    pub fn try_receive(&mut self) -> Option<NoteCommand> {
        self.consumer.pop().ok()
    }
}

/// A note command scheduled for an absolute position in the audio stream.
///
/// `frame` is measured in output audio frames, not individual channel samples
/// or callback-relative offsets. Producers should enqueue scheduled commands
/// in nondecreasing frame order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScheduledNoteCommand {
    /// Absolute output frame at which the command should take effect.
    pub frame: u64,
    /// Note operation to perform at the scheduled frame.
    pub command: NoteCommand,
}

/// The sending endpoint of a scheduled note-command queue.
pub struct ScheduledNoteCommandSender {
    producer: Producer<ScheduledNoteCommand>,
    last_frame: Option<u64>,
}

/// Reasons a scheduled command could not be enqueued.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScheduledNoteSendError {
    /// The command's frame precedes the last accepted command's frame.
    OutOfOrder {
        /// The command that was rejected.
        command: ScheduledNoteCommand,
        /// The frame of the last successfully enqueued command.
        last_frame: u64,
    },
    /// The bounded queue has no available capacity.
    Full(ScheduledNoteCommand),
}

/// The receiving endpoint of a scheduled note-command queue.
pub struct ScheduledNoteCommandReceiver {
    consumer: Consumer<ScheduledNoteCommand>,
}

/// Creates a bounded queue for timestamped note commands.
///
/// # Errors
///
/// Returns an error when `capacity` is zero.
pub fn scheduled_note_command_channel(
    capacity: usize,
) -> Result<(ScheduledNoteCommandSender, ScheduledNoteCommandReceiver), &'static str> {
    if capacity == 0 {
        return Err("Scheduled note command queue capacity must be greater than zero.");
    }

    let (producer, consumer) = RingBuffer::new(capacity);

    Ok((
        ScheduledNoteCommandSender {
            producer,
            last_frame: None,
        },
        ScheduledNoteCommandReceiver { consumer },
    ))
}

impl ScheduledNoteCommandSender {
    /// Attempts to enqueue a scheduled command without waiting.
    ///
    /// Requires commands to be submitted in nondecreasing frame order.
    ///
    /// Returns an error containing the original command if its frame is
    /// earlier than the last accepted frame or the queue is full.
    ///
    /// # Errors
    ///
    /// Returns [`ScheduledNoteSendError::OutOfOrder`] if the command's frame
    /// precedes the last successfully enqueued frame, or
    /// [`ScheduledNoteSendError::Full`] if the bounded queue has no capacity.
    pub fn try_send(
        &mut self,
        command: ScheduledNoteCommand,
    ) -> Result<(), ScheduledNoteSendError> {
        if let Some(last_frame) = self.last_frame {
            if command.frame < last_frame {
                return Err(ScheduledNoteSendError::OutOfOrder {
                    command,
                    last_frame,
                });
            }
        }

        match self.producer.push(command) {
            Ok(()) => {
                self.last_frame = Some(command.frame);
                Ok(())
            }
            Err(PushError::Full(command)) => Err(ScheduledNoteSendError::Full(command)),
        }
    }
}

impl ScheduledNoteCommandReceiver {
    /// Attempts to receive one scheduled command without waiting.
    ///
    /// Returns `None` when the queue is empty.
    #[must_use]
    pub fn try_receive(&mut self) -> Option<ScheduledNoteCommand> {
        self.consumer.pop().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NoteCommand, ScheduledNoteCommand, ScheduledNoteSendError, note_command_channel,
        scheduled_note_command_channel,
    };
    use crate::oscillator::Waveform;

    fn note_on(voice_id: u64, frequency_hz: f32) -> NoteCommand {
        NoteCommand::NoteOn {
            voice_id,
            frequency_hz,
            gain: 0.5,
        }
    }

    #[test]
    fn scheduled_command_channel_rejects_zero_capacity() {
        assert!(scheduled_note_command_channel(0).is_err());
    }

    #[test]
    fn scheduled_command_channel_preserves_frame_positions_and_fifo_order() {
        let (mut sender, mut receiver) =
            scheduled_note_command_channel(4).expect("valid queue capacity");

        let first = ScheduledNoteCommand {
            frame: 128,
            command: note_on(1, 440.0),
        };
        let second = ScheduledNoteCommand {
            frame: 256,
            command: NoteCommand::NoteOff { voice_id: 1 },
        };
        let third = ScheduledNoteCommand {
            frame: 256,
            command: note_on(2, 660.0),
        };

        assert_eq!(sender.try_send(first), Ok(()));
        assert_eq!(sender.try_send(second), Ok(()));
        assert_eq!(sender.try_send(third), Ok(()));

        assert_eq!(receiver.try_receive(), Some(first));
        assert_eq!(receiver.try_receive(), Some(second));
        assert_eq!(receiver.try_receive(), Some(third));
        assert_eq!(receiver.try_receive(), None);
    }

    #[test]
    fn scheduled_command_sender_rejects_out_of_order_frames() {
        let (mut sender, mut receiver) =
            scheduled_note_command_channel(4).expect("valid queue capacity");

        let accepted = ScheduledNoteCommand {
            frame: 256,
            command: note_on(1, 440.0),
        };
        let rejected = ScheduledNoteCommand {
            frame: 128,
            command: NoteCommand::NoteOff { voice_id: 1 },
        };

        assert_eq!(sender.try_send(accepted), Ok(()));
        assert_eq!(
            sender.try_send(rejected),
            Err(ScheduledNoteSendError::OutOfOrder {
                command: rejected,
                last_frame: 256,
            })
        );

        assert_eq!(receiver.try_receive(), Some(accepted));
        assert_eq!(receiver.try_receive(), None);
    }

    #[test]
    fn scheduled_command_queue_returns_rejected_command_when_full() {
        let (mut sender, mut receiver) =
            scheduled_note_command_channel(1).expect("valid queue capacity");

        let accepted = ScheduledNoteCommand {
            frame: 64,
            command: note_on(1, 440.0),
        };
        let rejected = ScheduledNoteCommand {
            frame: 128,
            command: NoteCommand::NoteOff { voice_id: 1 },
        };

        assert_eq!(sender.try_send(accepted), Ok(()));
        assert_eq!(
            sender.try_send(rejected),
            Err(ScheduledNoteSendError::Full(rejected))
        );
        assert_eq!(receiver.try_receive(), Some(accepted));
    }

    #[test]
    fn command_channel_rejects_zero_capacity() {
        assert!(note_command_channel(0).is_err());
    }

    #[test]
    fn command_channel_preserves_fifo_order() {
        let (mut sender, mut receiver) = note_command_channel(4).expect("valid queue capacity");

        let first = note_on(1, 440.0);
        let second = NoteCommand::NoteOff { voice_id: 1 };
        let third = note_on(2, 660.0);

        assert_eq!(sender.try_send(first), Ok(()));
        assert_eq!(sender.try_send(second), Ok(()));
        assert_eq!(sender.try_send(third), Ok(()));

        assert_eq!(receiver.try_receive(), Some(first));
        assert_eq!(receiver.try_receive(), Some(second));
        assert_eq!(receiver.try_receive(), Some(third));
        assert_eq!(receiver.try_receive(), None);
    }

    #[test]
    fn command_channel_preserves_waveform_note_on_commands() {
        let (mut sender, mut receiver) = note_command_channel(1).expect("valid queue capacity");
        let command = NoteCommand::NoteOnWithWaveform {
            voice_id: 7,
            frequency_hz: 330.0,
            gain: 0.4,
            waveform: Waveform::Triangle,
        };

        assert_eq!(sender.try_send(command), Ok(()));
        assert_eq!(receiver.try_receive(), Some(command));
    }

    #[test]
    fn full_queue_returns_the_rejected_command() {
        let (mut sender, mut receiver) = note_command_channel(1).expect("valid queue capacity");

        let accepted = note_on(1, 440.0);
        let rejected = note_on(2, 660.0);

        assert_eq!(sender.try_send(accepted), Ok(()));
        assert_eq!(sender.try_send(rejected), Err(rejected));
        assert_eq!(receiver.try_receive(), Some(accepted));
    }

    #[test]
    fn queue_can_be_reused_after_receiving() {
        let (mut sender, mut receiver) = note_command_channel(1).expect("valid queue capacity");

        let first = note_on(1, 440.0);
        let second = note_on(2, 880.0);

        assert_eq!(sender.try_send(first), Ok(()));
        assert_eq!(receiver.try_receive(), Some(first));
        assert_eq!(sender.try_send(second), Ok(()));
        assert_eq!(receiver.try_receive(), Some(second));
    }
}
