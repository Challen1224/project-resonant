//! Audio output device discovery and configuration inspection.

use crate::command::{
    NoteCommand, NoteCommandReceiver, ScheduledNoteCommand, ScheduledNoteCommandSender,
    ScheduledNoteSendError, scheduled_note_command_channel,
};
use crate::envelope::AdsrParameters;
use crate::mixer::AudioMixer;
use crate::oscillator::SineOscillator;
use crate::scheduler::ScheduledNoteRenderer;
use crate::voice_manager::AudioVoiceManager;
use cpal::traits::StreamTrait;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// Information about an available audio output device.
#[derive(Clone, Debug)]
pub struct OutputDeviceInfo {
    /// Human-readable device description.
    pub name: String,
    /// Whether this is the system's default output device.
    pub is_default: bool,
    /// Default output sample rate, in samples per second.
    pub sample_rate_hz: u32,
    /// Number of output channels.
    pub channels: u16,
    /// Default output sample format.
    pub sample_format: String,
}

/// Enumerates available audio output devices.
///
/// Device-specific errors are returned as strings so callers can report
/// them without depending on CPAL's error types.
///
/// # Errors
///
/// Returns an error if output devices cannot be enumerated or a device's
/// description or default output configuration cannot be read.
pub fn enumerate_output_devices() -> Result<Vec<OutputDeviceInfo>, String> {
    let host = cpal::default_host();

    let default_device = host.default_output_device();
    let default_name = default_device
        .as_ref()
        .and_then(|device| device.description().ok())
        .map(|description| description.to_string());

    let devices = host
        .output_devices()
        .map_err(|error| format!("Failed to enumerate output devices: {error}"))?;

    let mut results = Vec::new();

    for device in devices {
        let description = device
            .description()
            .map_err(|error| format!("Failed to read device description: {error}"))?;

        let name = description.to_string();

        let config = device
            .default_output_config()
            .map_err(|error| format!("Failed to read default configuration for {name}: {error}"))?;

        let is_default = default_name.as_deref() == Some(name.as_str());

        results.push(OutputDeviceInfo {
            name,
            is_default,
            sample_rate_hz: config.sample_rate(),
            channels: config.channels(),
            sample_format: format!("{:?}", config.sample_format()),
        });
    }

    Ok(results)
}

/// Shared parameters published by the control thread and read by the audio callback.
struct AudioParameters {
    frequency_hz: AtomicU32,
    amplitude: AtomicU32,
    sample_rate_hz: f32,
}

impl AudioParameters {
    fn new(sample_rate_hz: f32, frequency_hz: f32, amplitude: f32) -> Self {
        Self {
            frequency_hz: AtomicU32::new(frequency_hz.to_bits()),
            amplitude: AtomicU32::new(amplitude.to_bits()),
            sample_rate_hz,
        }
    }

    fn set_frequency_hz(&self, frequency_hz: f32) -> bool {
        if !frequency_hz.is_finite()
            || frequency_hz < 0.0
            || frequency_hz > self.sample_rate_hz / 2.0
        {
            return false;
        }

        self.frequency_hz
            .store(frequency_hz.to_bits(), Ordering::Relaxed);
        true
    }

    fn set_amplitude(&self, amplitude: f32) -> bool {
        if !amplitude.is_finite() || !(0.0..=1.0).contains(&amplitude) {
            return false;
        }

        self.amplitude.store(amplitude.to_bits(), Ordering::Relaxed);
        true
    }

    fn frequency_hz(&self) -> f32 {
        f32::from_bits(self.frequency_hz.load(Ordering::Relaxed))
    }

    fn amplitude(&self) -> f32 {
        f32::from_bits(self.amplitude.load(Ordering::Relaxed))
    }
}

/// A running audio output stream.
///
/// Keeping this value alive keeps the underlying audio stream active.
pub struct AudioOutput {
    _stream: Stream,
    parameters: Arc<AudioParameters>,
}

impl AudioOutput {
    /// Opens the default output device and starts a sine-wave stream.
    ///
    /// # Errors
    ///
    /// Returns an error if no default output device exists, its configuration
    /// cannot be read, its sample format is unsupported, or the stream cannot
    /// be built or started.
    pub fn start_sine(frequency_hz: f32, amplitude: f32) -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| "No default audio output device is available.".to_string())?;

        let supported_config = device
            .default_output_config()
            .map_err(|error| format!("Failed to read output configuration: {error}"))?;

        if supported_config.sample_format() != SampleFormat::F32 {
            return Err(format!(
                "Unsupported output sample format: {:?}. This prototype currently supports F32.",
                supported_config.sample_format()
            ));
        }

        let config: StreamConfig = supported_config.into();
        // Standard audio sample rates are exactly representable as f32.
        #[allow(clippy::cast_precision_loss)]
        let sample_rate = config.sample_rate as f32;
        let channels = usize::from(config.channels);

        let mut oscillator = SineOscillator::new(frequency_hz, sample_rate, amplitude)
            .ok_or_else(|| "Invalid sine oscillator configuration.".to_string())?;

        let parameters = Arc::new(AudioParameters::new(sample_rate, frequency_hz, amplitude));
        let callback_parameters = Arc::clone(&parameters);

        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    let frequency_hz = callback_parameters.frequency_hz();
                    let amplitude = callback_parameters.amplitude();

                    // Parameters are validated before publication by the control thread.
                    let _ = oscillator.set_frequency_hz(frequency_hz);
                    let _ = oscillator.set_amplitude(amplitude);

                    for frame in data.chunks_mut(channels) {
                        let sample = oscillator.next_sample();
                        for channel in frame {
                            *channel = sample;
                        }
                    }
                },
                |error| eprintln!("Audio stream error: {error}"),
                None,
            )
            .map_err(|error| format!("Failed to build audio output stream: {error}"))?;

        stream
            .play()
            .map_err(|error| format!("Failed to start audio output stream: {error}"))?;

        Ok(Self {
            _stream: stream,
            parameters,
        })
    }

    /// Changes playback frequency without restarting the audio stream.
    ///
    /// Returns `false` if the frequency is invalid or exceeds Nyquist.
    #[must_use]
    pub fn set_frequency_hz(&self, frequency_hz: f32) -> bool {
        self.parameters.set_frequency_hz(frequency_hz)
    }

    /// Changes playback amplitude without restarting the audio stream.
    ///
    /// Returns `false` if the amplitude is outside 0..=1.
    #[must_use]
    pub fn set_amplitude(&self, amplitude: f32) -> bool {
        self.parameters.set_amplitude(amplitude)
    }
}

/// A running audio output stream driven by an audio mixer.
///
/// The mixer is owned by the audio callback for the lifetime of the stream.
/// Keeping this value alive keeps the underlying audio stream active.
pub struct AudioMixerOutput {
    _stream: Stream,
}

impl AudioMixerOutput {
    /// Opens the default output device and starts rendering the supplied mixer.
    ///
    /// The mixer should be fully configured before it is passed here.
    /// Its voices are rendered on the audio callback thread.
    ///
    /// # Errors
    ///
    /// Returns an error if no default output device exists, its configuration
    /// cannot be read, its sample format is unsupported, or the stream cannot
    /// be built or started.
    pub fn start(mut mixer: AudioMixer) -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| "No default audio output device is available.".to_string())?;

        let supported_config = device
            .default_output_config()
            .map_err(|error| format!("Failed to read output configuration: {error}"))?;

        if supported_config.sample_format() != SampleFormat::F32 {
            return Err(format!(
                "Unsupported output sample format: {:?}. This prototype currently supports F32.",
                supported_config.sample_format()
            ));
        }

        let config: StreamConfig = supported_config.into();
        let channels = usize::from(config.channels);

        if channels == 0 {
            return Err("The output device reported zero channels.".to_string());
        }

        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    for frame in data.chunks_mut(channels) {
                        let sample = mixer.next_sample();

                        for channel in frame {
                            *channel = sample;
                        }
                    }
                },
                |error| eprintln!("Audio mixer stream error: {error}"),
                None,
            )
            .map_err(|error| format!("Failed to build audio mixer output stream: {error}"))?;

        stream
            .play()
            .map_err(|error| format!("Failed to start audio mixer output stream: {error}"))?;

        Ok(Self { _stream: stream })
    }
}

/// A running audio output stream controlled by real-time note commands.
///
/// The audio callback owns the voice manager and consumes commands from a
/// bounded single-producer, single-consumer queue. The producer never needs
/// to acquire a lock or wait for the audio callback.
pub struct AudioCommandOutput {
    _stream: Stream,
}

impl AudioCommandOutput {
    /// Opens the default output device and starts a command-driven audio stream.
    ///
    /// The voice manager is initialized using the output device's actual
    /// sample rate. Its voice storage is allocated before the callback starts.
    ///
    /// Commands are consumed at the beginning of each audio buffer. Invalid
    /// Note On commands and commands exceeding voice capacity are discarded.
    ///
    /// # Errors
    ///
    /// Returns an error if the queue output cannot be initialized, no default
    /// output device exists, its configuration cannot be read, its sample
    /// format is unsupported, or the stream cannot be built or started.
    pub fn start(receiver: NoteCommandReceiver, max_voices: usize) -> Result<Self, String> {
        let envelope_parameters = AdsrParameters::new(0.0, 0.0, 1.0, 0.0)
            .ok_or_else(|| "Invalid default envelope configuration.".to_string())?;

        Self::start_with_envelope(receiver, max_voices, envelope_parameters)
    }

    /// Opens the default output device with configurable voice envelopes.
    ///
    /// The voice manager uses the output device's actual sample rate, and all
    /// voice storage is allocated before the audio callback starts.
    ///
    /// # Errors
    ///
    /// Returns the same initialization errors as [`Self::start`], or an error
    /// when the envelope configuration is invalid for the device sample rate.
    pub fn start_with_envelope(
        mut receiver: NoteCommandReceiver,
        max_voices: usize,
        envelope_parameters: AdsrParameters,
    ) -> Result<Self, String> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| "No default audio output device is available.".to_string())?;

        let supported_config = device
            .default_output_config()
            .map_err(|error| format!("Failed to read output configuration: {error}"))?;

        if supported_config.sample_format() != SampleFormat::F32 {
            return Err(format!(
                "Unsupported output sample format: {:?}. This prototype currently supports F32.",
                supported_config.sample_format()
            ));
        }

        let config: StreamConfig = supported_config.into();
        let channels = usize::from(config.channels);

        if channels == 0 {
            return Err("The output device reported zero channels.".to_string());
        }

        // Standard audio sample rates are exactly representable as f32.
        #[allow(clippy::cast_precision_loss)]
        let sample_rate_hz = config.sample_rate as f32;

        let mut manager =
            AudioVoiceManager::new_with_envelope(max_voices, sample_rate_hz, envelope_parameters)
                .ok_or_else(|| "Invalid audio voice manager configuration.".to_string())?;

        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    // Drain pending commands without blocking or allocating.
                    while let Some(command) = receiver.try_receive() {
                        match command {
                            NoteCommand::NoteOn {
                                voice_id,
                                frequency_hz,
                                gain,
                            } => {
                                let _ = manager.note_on(voice_id, frequency_hz, gain);
                            }
                            NoteCommand::NoteOnWithWaveform {
                                voice_id,
                                frequency_hz,
                                gain,
                                waveform,
                            } => {
                                let _ = manager.note_on_with_waveform(
                                    voice_id,
                                    frequency_hz,
                                    gain,
                                    waveform,
                                );
                            }
                            NoteCommand::NoteOff { voice_id } => {
                                let _ = manager.note_off(voice_id);
                            }
                        }
                    }

                    // Render the active voices into every output channel.
                    for frame in data.chunks_mut(channels) {
                        let sample = manager.next_sample();

                        for channel in frame {
                            *channel = sample;
                        }
                    }
                },
                |error| eprintln!("Audio command stream error: {error}"),
                None,
            )
            .map_err(|error| format!("Failed to build audio command stream: {error}"))?;

        stream
            .play()
            .map_err(|error| format!("Failed to start audio command stream: {error}"))?;

        Ok(Self { _stream: stream })
    }
}

/// A running audio output stream driven by absolute-frame note commands.
///
/// The audio callback owns the scheduled renderer. The producer submits
/// timestamped commands through a bounded, nonblocking SPSC queue.
///
/// The frame clock reports the next absolute output frame to be rendered.
/// Producers should schedule events sufficiently ahead of that position
/// to allow the callback to receive them before their deadlines.
pub struct ScheduledAudioCommandOutput {
    _stream: Stream,
    sender: ScheduledNoteCommandSender,
    current_frame: Arc<AtomicU64>,
}

impl ScheduledAudioCommandOutput {
    /// Opens the default output device and starts scheduled command playback.
    ///
    /// The renderer starts at absolute frame zero. Queue and voice storage
    /// are initialized before the audio callback starts.
    ///
    /// # Errors
    ///
    /// Returns an error if the queue capacity is zero, no default output
    /// device exists, its configuration cannot be read, its sample format
    /// is unsupported, or the stream cannot be built or started.
    pub fn start(max_voices: usize, queue_capacity: usize) -> Result<Self, String> {
        let envelope_parameters = AdsrParameters::new(0.0, 0.0, 1.0, 0.0)
            .ok_or_else(|| "Invalid default envelope configuration.".to_string())?;

        Self::start_with_envelope(max_voices, queue_capacity, envelope_parameters)
    }

    /// Opens the default output device with configurable voice envelopes.
    ///
    /// The frame clock begins at zero and advances once per rendered output
    /// frame, regardless of the number of channels or callback buffer size.
    ///
    /// # Errors
    ///
    /// Returns the same initialization errors as [`Self::start`], or an
    /// error when the envelope configuration is invalid for the device.
    pub fn start_with_envelope(
        max_voices: usize,
        queue_capacity: usize,
        envelope_parameters: AdsrParameters,
    ) -> Result<Self, String> {
        let (sender, receiver) =
            scheduled_note_command_channel(queue_capacity).map_err(str::to_string)?;

        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| "No default audio output device is available.".to_string())?;

        let supported_config = device
            .default_output_config()
            .map_err(|error| format!("Failed to read output configuration: {error}"))?;

        if supported_config.sample_format() != SampleFormat::F32 {
            return Err(format!(
                "Unsupported output sample format: {:?}. This prototype currently supports F32.",
                supported_config.sample_format()
            ));
        }

        let config: StreamConfig = supported_config.into();
        let channels = usize::from(config.channels);

        if channels == 0 {
            return Err("The output device reported zero channels.".to_string());
        }

        // Standard audio sample rates are exactly representable as f32.
        #[allow(clippy::cast_precision_loss)]
        let sample_rate_hz = config.sample_rate as f32;

        let voice_manager =
            AudioVoiceManager::new_with_envelope(max_voices, sample_rate_hz, envelope_parameters)
                .ok_or_else(|| "Invalid audio voice manager configuration.".to_string())?;

        let mut renderer = ScheduledNoteRenderer::new(receiver, voice_manager, 0);

        let current_frame = Arc::new(AtomicU64::new(0));
        let callback_frame = Arc::clone(&current_frame);

        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    for frame in data.chunks_mut(channels) {
                        let sample = renderer.next_sample();

                        for channel in frame {
                            *channel = sample;
                        }
                    }

                    // Publish once per completed callback buffer rather than
                    // performing an atomic store for every output frame.
                    callback_frame.store(renderer.next_frame(), Ordering::Release);
                },
                |error| eprintln!("Scheduled audio command stream error: {error}"),
                None,
            )
            .map_err(|error| format!("Failed to build scheduled audio command stream: {error}"))?;

        stream
            .play()
            .map_err(|error| format!("Failed to start scheduled audio command stream: {error}"))?;

        Ok(Self {
            _stream: stream,
            sender,
            current_frame,
        })
    }

    /// Attempts to enqueue a command at its absolute output frame.
    ///
    /// Commands must be submitted in nondecreasing frame order. This method
    /// does not wait for the audio callback.
    ///
    /// # Errors
    ///
    /// Returns an error containing the original command if its timestamp
    /// precedes the last accepted timestamp or the queue is full.
    pub fn try_schedule(
        &mut self,
        command: ScheduledNoteCommand,
    ) -> Result<(), ScheduledNoteSendError> {
        self.sender.try_send(command)
    }

    /// Returns the next absolute output frame reported by the audio callback.
    ///
    /// This value may advance concurrently while the stream is running.
    #[must_use]
    pub fn current_frame(&self) -> u64 {
        self.current_frame.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::AudioParameters;

    #[test]
    fn audio_parameters_accept_valid_updates() {
        let parameters = AudioParameters::new(48_000.0, 440.0, 0.2);

        assert!(parameters.set_frequency_hz(880.0));
        assert!(parameters.set_amplitude(0.7));
        assert_eq!(parameters.frequency_hz(), 880.0);
        assert_eq!(parameters.amplitude(), 0.7);
    }

    #[test]
    fn audio_parameters_reject_invalid_updates_without_changing_values() {
        let parameters = AudioParameters::new(48_000.0, 440.0, 0.2);

        assert!(!parameters.set_frequency_hz(f32::NAN));
        assert!(!parameters.set_frequency_hz(f32::INFINITY));
        assert!(!parameters.set_frequency_hz(-1.0));
        assert!(!parameters.set_frequency_hz(24_001.0));
        assert!(!parameters.set_amplitude(f32::NAN));
        assert!(!parameters.set_amplitude(f32::INFINITY));
        assert!(!parameters.set_amplitude(-0.1));
        assert!(!parameters.set_amplitude(1.1));

        assert_eq!(parameters.frequency_hz(), 440.0);
        assert_eq!(parameters.amplitude(), 0.2);
    }
}
