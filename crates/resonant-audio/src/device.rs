//! Audio output device discovery and configuration inspection.

use crate::oscillator::SineOscillator;
use cpal::traits::StreamTrait;
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SampleFormat, Stream, StreamConfig};

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

/// A running audio output stream.
///
/// Keeping this value alive keeps the underlying audio stream active.
pub struct AudioOutput {
    _stream: Stream,
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

        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
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

        Ok(Self { _stream: stream })
    }
}
