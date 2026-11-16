//! Reusable audio voice primitives.

use crate::envelope::{AdsrEnvelope, AdsrParameters};
use crate::oscillator::SineOscillator;

/// A playable audio voice with an oscillator, gain, and amplitude envelope.
///
/// A voice owns its DSP state and performs no allocation while rendering.
#[derive(Clone, Copy, Debug)]
pub struct AudioVoice {
    oscillator: SineOscillator,
    envelope: AdsrEnvelope,
    gain: f32,
}

impl AudioVoice {
    /// Creates an immediate full-level voice with the requested frequency,
    /// sample rate, and gain.
    ///
    /// Gain must be finite and within `0.0..=1.0`.
    /// Returns `None` if any configuration value is invalid.
    #[must_use]
    pub fn new(frequency_hz: f32, sample_rate_hz: f32, gain: f32) -> Option<Self> {
        let envelope_parameters = AdsrParameters::new(0.0, 0.0, 1.0, 0.0)?;

        Self::new_with_envelope(frequency_hz, sample_rate_hz, gain, envelope_parameters)
    }

    /// Creates a voice with custom ADSR envelope parameters.
    ///
    /// The envelope begins its attack stage immediately.
    #[must_use]
    pub fn new_with_envelope(
        frequency_hz: f32,
        sample_rate_hz: f32,
        gain: f32,
        envelope_parameters: AdsrParameters,
    ) -> Option<Self> {
        if !gain.is_finite() || !(0.0..=1.0).contains(&gain) {
            return None;
        }

        let oscillator = SineOscillator::new(frequency_hz, sample_rate_hz, 1.0)?;
        let mut envelope = AdsrEnvelope::new(envelope_parameters, sample_rate_hz)?;
        envelope.note_on();

        Some(Self {
            oscillator,
            envelope,
            gain,
        })
    }

    /// Changes the voice frequency without resetting oscillator phase.
    ///
    /// Returns `false` if the frequency is invalid or exceeds Nyquist.
    pub fn set_frequency_hz(&mut self, frequency_hz: f32) -> bool {
        self.oscillator.set_frequency_hz(frequency_hz)
    }

    /// Changes the voice gain.
    ///
    /// Returns `false` if the gain is non-finite or outside `0.0..=1.0`.
    pub fn set_gain(&mut self, gain: f32) -> bool {
        if !gain.is_finite() || !(0.0..=1.0).contains(&gain) {
            return false;
        }

        self.gain = gain;
        true
    }

    /// Begins the envelope's release stage.
    pub fn note_off(&mut self) {
        self.envelope.note_off();
    }

    /// Returns whether the envelope is still active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.envelope.is_active()
    }

    /// Returns whether the voice is currently in its release stage.
    #[must_use]
    pub fn is_releasing(&self) -> bool {
        self.envelope.is_releasing()
    }

    /// Generates the next sample for this voice.
    #[must_use]
    pub fn next_sample(&mut self) -> f32 {
        self.oscillator.next_sample() * self.gain * self.envelope.next_sample()
    }
}

#[cfg(test)]
mod tests {
    use super::AudioVoice;
    use crate::envelope::AdsrParameters;
    use crate::oscillator::SineOscillator;

    #[test]
    fn voice_starts_at_zero_phase() {
        let mut voice = AudioVoice::new(440.0, 48_000.0, 0.5).expect("valid voice configuration");

        assert_eq!(voice.next_sample(), 0.0);
    }

    #[test]
    fn voice_applies_gain_to_generated_samples() {
        let mut voice = AudioVoice::new(440.0, 48_000.0, 0.5).expect("valid voice configuration");
        let mut reference =
            SineOscillator::new(440.0, 48_000.0, 1.0).expect("valid oscillator configuration");

        let _ = voice.next_sample();
        let _ = reference.next_sample();

        let voice_sample = voice.next_sample();
        let reference_sample = reference.next_sample();

        assert_eq!(voice_sample, reference_sample * 0.5);
    }

    #[test]
    fn voice_frequency_can_change() {
        let mut voice = AudioVoice::new(440.0, 48_000.0, 0.5).expect("valid voice configuration");

        assert!(voice.set_frequency_hz(880.0));
        assert!(!voice.set_frequency_hz(24_001.0));
    }

    #[test]
    fn voice_gain_rejects_invalid_updates_without_changing_value() {
        let mut voice = AudioVoice::new(440.0, 48_000.0, 0.5).expect("valid voice configuration");

        assert!(!voice.set_gain(f32::NAN));
        assert!(!voice.set_gain(f32::INFINITY));
        assert!(!voice.set_gain(-0.1));
        assert!(!voice.set_gain(1.1));
        assert_eq!(voice.gain, 0.5);

        assert!(voice.set_gain(0.75));
        assert_eq!(voice.gain, 0.75);
    }

    #[test]
    fn voice_rejects_invalid_initial_configuration() {
        assert!(AudioVoice::new(-1.0, 48_000.0, 0.5).is_none());
        assert!(AudioVoice::new(440.0, 0.0, 0.5).is_none());
        assert!(AudioVoice::new(440.0, 48_000.0, f32::NAN).is_none());
        assert!(AudioVoice::new(440.0, 48_000.0, 1.5).is_none());
    }

    #[test]
    fn voice_supports_custom_envelope_parameters() {
        let envelope =
            AdsrParameters::new(0.002, 0.002, 0.5, 0.002).expect("valid envelope parameters");

        let mut voice = AudioVoice::new_with_envelope(440.0, 1_000.0, 0.5, envelope)
            .expect("valid voice configuration");

        assert!(voice.is_active());

        let _ = voice.next_sample();
        let _ = voice.next_sample();
        let _ = voice.next_sample();
        let _ = voice.next_sample();

        voice.note_off();

        assert!(voice.is_active());
        let _ = voice.next_sample();
        let _ = voice.next_sample();
        assert!(!voice.is_active());
    }
}
