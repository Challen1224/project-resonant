//! Audio output device discovery and configuration inspection.

use crate::oscillator::SineOscillator;
use cpal::traits::StreamTrait;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleFormat, Stream, StreamConfig};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

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
