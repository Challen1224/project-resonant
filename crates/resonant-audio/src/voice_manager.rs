//! Fixed-capacity voice management for real-time audio rendering.

use crate::envelope::{AdsrEnvelope, AdsrParameters};
use crate::oscillator::Waveform;
use crate::sample::{SampleBank, SampleVoice};
use crate::voice::AudioVoice;

/// An active voice paired with its application-assigned identifier.
#[derive(Debug)]
enum VoiceSource {
    Synth(AudioVoice),
    Sample { voice: SampleVoice, sample_id: u32 },
}

#[derive(Debug)]
struct ActiveVoice {
    id: u64,
    source: VoiceSource,
}

/// Manages a fixed number of audio voices without allocating during playback.
///
/// Voices remain allocated during their release stage and are reclaimed
/// automatically when their envelopes become inactive.
#[derive(Debug)]
pub struct AudioVoiceManager {
    voices: Vec<Option<ActiveVoice>>,
    sample_rate_hz: f32,
    envelope_parameters: AdsrParameters,
    sample_bank: SampleBank,
    active_voice_count: usize,
}

impl AudioVoiceManager {
    /// Creates a voice manager with immediate attack, sustain, and release.
    ///
    /// This preserves the original behavior for existing callers.
    ///
    /// # Errors
    ///
    /// Returns `None` if the sample rate is invalid or capacity is zero.
    #[must_use]
    pub fn new(max_voices: usize, sample_rate_hz: f32) -> Option<Self> {
        let envelope_parameters = AdsrParameters::new(0.0, 0.0, 1.0, 0.0)?;

        Self::new_with_envelope(max_voices, sample_rate_hz, envelope_parameters)
    }

    /// Creates a voice manager with configurable ADSR envelope parameters.
    ///
    /// # Errors
    ///
    /// Returns `None` if capacity, sample rate, or envelope configuration
    /// is invalid.
    #[must_use]
    pub fn new_with_envelope(
        max_voices: usize,
        sample_rate_hz: f32,
        envelope_parameters: AdsrParameters,
    ) -> Option<Self> {
        if max_voices == 0
            || !sample_rate_hz.is_finite()
            || sample_rate_hz <= 0.0
            || sample_rate_hz > 768_000.0
        {
            return None;
        }

        AdsrEnvelope::new(envelope_parameters, sample_rate_hz)?;

        let voices = std::iter::repeat_with(|| None).take(max_voices).collect();

        Some(Self {
            voices,
            sample_rate_hz,
            envelope_parameters,
            sample_bank: SampleBank::new(),
            active_voice_count: 0,
        })
    }

    /// Creates a voice manager with a preloaded sample bank.
    ///
    /// The bank is moved into the manager before audio rendering begins.
    #[must_use]
    pub fn new_with_envelope_and_samples(
        max_voices: usize,
        sample_rate_hz: f32,
        envelope_parameters: AdsrParameters,
        sample_bank: SampleBank,
    ) -> Option<Self> {
        let mut manager = Self::new_with_envelope(max_voices, sample_rate_hz, envelope_parameters)?;
        manager.sample_bank = sample_bank;
        Some(manager)
    }

    /// Returns the number of allocated voices, including voices in release.
    #[must_use]
    pub fn voice_count(&self) -> usize {
        self.active_voice_count
    }

    /// Returns the manager's fixed voice capacity.
    #[must_use]
    pub fn voice_capacity(&self) -> usize {
        self.voices.len()
    }

    /// Adds mono audio to the preloaded sample bank.
    ///
    /// Call this before starting the audio stream.
    pub fn add_sample(&mut self, sample_rate_hz: u32, samples: Vec<f32>) -> Option<u32> {
        self.sample_bank.add_sample(sample_rate_hz, samples)
    }

    /// Starts or replaces a one-shot sample voice.
    ///
    /// Returns false for an unknown sample, invalid gain, or a full voice pool.
    pub fn sample_on(&mut self, voice_id: u64, sample_id: u32, gain: f32) -> bool {
        let Some(sample) = self.sample_bank.get(sample_id) else {
            return false;
        };

        let Some(voice) = ({
            // The manager validates this rate is finite and within 1..=768 kHz.
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "The validated positive output rate is rounded to integer hertz."
            )]
            let output_sample_rate_hz = self.sample_rate_hz.round() as u32;
            SampleVoice::new(sample.sample_rate_hz(), output_sample_rate_hz, gain)
        }) else {
            return false;
        };

        let source = VoiceSource::Sample { voice, sample_id };

        if let Some(slot) = self
            .voices
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|active| active.id == voice_id))
        {
            *slot = Some(ActiveVoice {
                id: voice_id,
                source,
            });
            return true;
        }

        let Some(slot) = self.voices.iter_mut().find(|slot| slot.is_none()) else {
            return false;
        };

        *slot = Some(ActiveVoice {
            id: voice_id,
            source,
        });
        self.active_voice_count += 1;
        true
    }

    /// Starts or replaces a voice with the supplied identifier.
    ///
    /// An existing voice with the same identifier is replaced. Returns
    /// `false` if the voice configuration is invalid or all slots are full.
    pub fn note_on(&mut self, voice_id: u64, frequency_hz: f32, gain: f32) -> bool {
        self.note_on_with_waveform(voice_id, frequency_hz, gain, Waveform::Sine)
    }

    /// Starts or replaces a voice using the supplied oscillator waveform.
    ///
    /// An existing voice with the same identifier is replaced. Returns
    /// `false` if the voice configuration is invalid or all slots are full.
    pub fn note_on_with_waveform(
        &mut self,
        voice_id: u64,
        frequency_hz: f32,
        gain: f32,
        waveform: Waveform,
    ) -> bool {
        let Some(voice) = AudioVoice::new_with_envelope_and_waveform(
            frequency_hz,
            self.sample_rate_hz,
            gain,
            self.envelope_parameters,
            waveform,
        ) else {
            return false;
        };

        if let Some(slot) = self
            .voices
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|active| active.id == voice_id))
        {
            *slot = Some(ActiveVoice {
                id: voice_id,
                source: VoiceSource::Synth(voice),
            });
            return true;
        }

        let Some(slot) = self.voices.iter_mut().find(|slot| slot.is_none()) else {
            return false;
        };

        *slot = Some(ActiveVoice {
            id: voice_id,
            source: VoiceSource::Synth(voice),
        });
        self.active_voice_count += 1;
        true
    }

    /// Begins the release stage for the voice with the supplied identifier.
    ///
    /// The voice continues rendering until its envelope reaches idle.
    /// Returns `true` when a matching voice was found.
    pub fn note_off(&mut self, voice_id: u64) -> bool {
        let Some(active) = self
            .voices
            .iter_mut()
            .filter_map(Option::as_mut)
            .find(|active| active.id == voice_id)
        else {
            return false;
        };

        let VoiceSource::Synth(voice) = &mut active.source else {
            return false;
        };

        if voice.is_releasing() || !voice.is_active() {
            return false;
        }

        voice.note_off();
        true
    }

    /// Immediately stops and removes the voice with the supplied identifier.
    ///
    /// Use this when an immediate termination is required instead of an
    /// envelope-controlled release.
    pub fn stop_voice(&mut self, voice_id: u64) -> bool {
        let Some(slot) = self
            .voices
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|active| active.id == voice_id))
        else {
            return false;
        };

        *slot = None;
        self.active_voice_count -= 1;
        true
    }

    /// Generates one sample by summing all allocated voices.
    ///
    /// Voices whose release envelopes have completed are reclaimed here.
    /// No allocation occurs during rendering.
    #[must_use]
    pub fn next_sample(&mut self) -> f32 {
        let mut mixed_sample = 0.0;

        for slot in &mut self.voices {
            let is_active = {
                let Some(active) = slot.as_mut() else {
                    continue;
                };

                match &mut active.source {
                    VoiceSource::Synth(voice) => {
                        mixed_sample += voice.next_sample();
                        voice.is_active()
                    }
                    VoiceSource::Sample { voice, sample_id } => {
                        let Some(sample) = self.sample_bank.get(*sample_id) else {
                            continue;
                        };

                        mixed_sample += voice.next_sample(sample.samples());
                        voice.is_active()
                    }
                }
            };

            if !is_active {
                *slot = None;
                self.active_voice_count -= 1;
            }
        }

        mixed_sample
    }
}

#[cfg(test)]
mod tests {
    use super::AudioVoiceManager;
    use crate::envelope::AdsrParameters;

    fn release_envelope() -> AdsrParameters {
        AdsrParameters::new(0.0, 0.0, 1.0, 0.002).expect("valid release envelope")
    }

    #[test]
    fn manager_starts_empty_with_fixed_capacity() {
        let manager = AudioVoiceManager::new(8, 48_000.0).expect("valid configuration");

        assert_eq!(manager.voice_count(), 0);
        assert_eq!(manager.voice_capacity(), 8);
    }

    #[test]
    fn manager_rejects_invalid_configuration() {
        assert!(AudioVoiceManager::new(0, 48_000.0).is_none());
        assert!(AudioVoiceManager::new(4, 0.0).is_none());
        assert!(AudioVoiceManager::new(4, f32::NAN).is_none());
        assert!(AudioVoiceManager::new(4, f32::INFINITY).is_none());
    }

    #[test]
    fn manager_rejects_invalid_envelope_configuration() {
        let invalid_envelope = AdsrParameters::new(0.0, 0.0, 1.5, 0.1);

        assert!(invalid_envelope.is_none());

        let invalid_sample_rate =
            AdsrParameters::new(0.0, 0.0, 1.0, 0.1).expect("valid envelope parameters");

        assert!(AudioVoiceManager::new_with_envelope(4, 0.0, invalid_sample_rate).is_none());
    }

    #[test]
    fn note_on_adds_voice_and_note_off_releases_it() {
        let mut manager = AudioVoiceManager::new(2, 48_000.0).expect("valid configuration");

        assert!(manager.note_on(10, 440.0, 0.5));
        assert_eq!(manager.voice_count(), 1);

        assert!(manager.note_off(10));
        assert_eq!(manager.voice_count(), 1);

        assert!(!manager.note_off(10));

        let _ = manager.next_sample();

        assert_eq!(manager.voice_count(), 0);
    }

    #[test]
    fn sample_voice_renders_preloaded_audio_and_reclaims_its_slot() {
        let mut manager = AudioVoiceManager::new(1, 48_000.0).expect("valid configuration");
        let sample_id = manager
            .add_sample(48_000, vec![0.25, -0.5, 0.75])
            .expect("valid sample");

        assert!(manager.sample_on(42, sample_id, 0.5));
        assert_eq!(manager.voice_count(), 1);
        assert_eq!(manager.next_sample(), 0.125);
        assert_eq!(manager.next_sample(), -0.25);
        assert_eq!(manager.next_sample(), 0.375);
        assert_eq!(manager.voice_count(), 0);
        assert_eq!(manager.next_sample(), 0.0);
    }

    #[test]
    fn sample_voice_rejects_unknown_samples_and_invalid_gain() {
        let mut manager = AudioVoiceManager::new(2, 48_000.0).expect("valid configuration");
        let sample_id = manager
            .add_sample(48_000, vec![0.5, 0.25])
            .expect("valid sample");

        assert!(!manager.sample_on(1, 99, 0.5));
        assert!(!manager.sample_on(1, sample_id, f32::NAN));
        assert!(!manager.sample_on(1, sample_id, 1.5));
        assert_eq!(manager.voice_count(), 0);
    }

    #[test]
    fn sample_voice_respects_capacity_and_replaces_matching_identifier() {
        let mut manager = AudioVoiceManager::new(1, 48_000.0).expect("valid configuration");
        let first_sample = manager
            .add_sample(48_000, vec![0.25, 0.5, 0.75])
            .expect("valid first sample");
        let second_sample = manager
            .add_sample(48_000, vec![-0.5, -0.25])
            .expect("valid second sample");

        assert!(manager.sample_on(1, first_sample, 1.0));
        assert!(!manager.sample_on(2, second_sample, 1.0));
        assert!(manager.sample_on(1, second_sample, 0.5));
        assert_eq!(manager.voice_count(), 1);
        assert_eq!(manager.next_sample(), -0.25);
    }

    #[test]
    fn manager_generates_samples_for_each_waveform() {
        use crate::oscillator::Waveform;

        let cases = [
            (Waveform::Sine, 0.0),
            (Waveform::Square, 0.5),
            (Waveform::Sawtooth, -0.5),
            (Waveform::Triangle, -0.5),
        ];

        for (waveform, expected) in cases {
            let mut manager = AudioVoiceManager::new(1, 4.0).expect("valid manager configuration");

            assert!(manager.note_on_with_waveform(1, 1.0, 0.5, waveform));
            assert_eq!(manager.next_sample(), expected, "waveform: {waveform:?}");
        }
    }

    #[test]
    fn released_voice_slot_is_reclaimed_after_release_finishes() {
        let mut manager = AudioVoiceManager::new_with_envelope(1, 1_000.0, release_envelope())
            .expect("valid configuration");

        assert!(manager.note_on(1, 440.0, 0.5));
        assert!(manager.note_off(1));
        assert_eq!(manager.voice_count(), 1);

        let _ = manager.next_sample();
        assert_eq!(manager.voice_count(), 1);

        let _ = manager.next_sample();
        assert_eq!(manager.voice_count(), 0);

        assert!(manager.note_on(2, 440.0, 0.5));
        assert_eq!(manager.voice_count(), 1);
    }

    #[test]
    fn manager_rejects_voice_when_capacity_is_full() {
        let mut manager = AudioVoiceManager::new(1, 48_000.0).expect("valid configuration");

        assert!(manager.note_on(1, 440.0, 0.5));
        assert!(!manager.note_on(2, 660.0, 0.5));
        assert_eq!(manager.voice_count(), 1);
    }

    #[test]
    fn note_on_replaces_voice_with_matching_identifier() {
        let mut manager = AudioVoiceManager::new(1, 48_000.0).expect("valid configuration");

        assert!(manager.note_on(1, 440.0, 0.5));
        assert!(manager.note_on(1, 880.0, 0.5));
        assert_eq!(manager.voice_count(), 1);
    }

    #[test]
    fn manager_rejects_invalid_voice_parameters() {
        let mut manager = AudioVoiceManager::new(2, 48_000.0).expect("valid configuration");

        assert!(!manager.note_on(1, -440.0, 0.5));
        assert!(!manager.note_on(1, 440.0, 2.0));
        assert_eq!(manager.voice_count(), 0);
    }

    #[test]
    fn manager_reuses_slots_after_immediate_stop() {
        let mut manager = AudioVoiceManager::new(1, 48_000.0).expect("valid configuration");

        assert!(manager.note_on(1, 440.0, 0.5));
        assert!(manager.stop_voice(1));
        assert_eq!(manager.voice_count(), 0);

        assert!(manager.note_on(2, 660.0, 0.5));
        assert_eq!(manager.voice_count(), 1);
    }

    #[test]
    fn manager_renders_silence_when_empty() {
        let mut manager = AudioVoiceManager::new(4, 48_000.0).expect("valid configuration");

        assert_eq!(manager.next_sample(), 0.0);
    }
}
