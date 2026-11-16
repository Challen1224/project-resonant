//! Fixed-capacity voice management for real-time audio rendering.

use crate::voice::AudioVoice;

/// An active voice paired with its application-assigned identifier.
#[derive(Debug)]
struct ActiveVoice {
    id: u64,
    voice: AudioVoice,
}

/// Manages a fixed number of audio voices without allocating during playback.
///
/// Construct this manager before starting the audio callback. Note handling
/// and sample rendering use only the storage allocated during construction.
#[derive(Debug)]
pub struct AudioVoiceManager {
    voices: Vec<Option<ActiveVoice>>,
    sample_rate_hz: f32,
    active_voice_count: usize,
}

impl AudioVoiceManager {
    /// Creates a voice manager with a fixed voice capacity.
    ///
    /// # Errors
    ///
    /// Returns `None` if the sample rate is invalid or capacity is zero.
    #[must_use]
    pub fn new(max_voices: usize, sample_rate_hz: f32) -> Option<Self> {
        if max_voices == 0 || !sample_rate_hz.is_finite() || sample_rate_hz <= 0.0 {
            return None;
        }

        let voices = std::iter::repeat_with(|| None).take(max_voices).collect();

        Some(Self {
            voices,
            sample_rate_hz,
            active_voice_count: 0,
        })
    }

    /// Returns the number of currently active voices.
    #[must_use]
    pub fn voice_count(&self) -> usize {
        self.active_voice_count
    }

    /// Returns the manager's fixed voice capacity.
    #[must_use]
    pub fn voice_capacity(&self) -> usize {
        self.voices.len()
    }

    /// Starts or replaces a voice with the supplied identifier.
    ///
    /// An existing voice with the same identifier is replaced. Returns
    /// `false` if the voice configuration is invalid or all slots are full.
    pub fn note_on(&mut self, voice_id: u64, frequency_hz: f32, gain: f32) -> bool {
        let Some(voice) = AudioVoice::new(frequency_hz, self.sample_rate_hz, gain) else {
            return false;
        };

        if let Some(slot) = self
            .voices
            .iter_mut()
            .find(|slot| slot.as_ref().is_some_and(|active| active.id == voice_id))
        {
            *slot = Some(ActiveVoice {
                id: voice_id,
                voice,
            });
            return true;
        }

        let Some(slot) = self.voices.iter_mut().find(|slot| slot.is_none()) else {
            return false;
        };

        *slot = Some(ActiveVoice {
            id: voice_id,
            voice,
        });
        self.active_voice_count += 1;
        true
    }

    /// Immediately stops the voice with the supplied identifier.
    ///
    /// Returns `true` when an active voice was found and stopped.
    pub fn note_off(&mut self, voice_id: u64) -> bool {
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

    /// Generates one sample by summing all active voices.
    ///
    /// No allocation occurs during rendering.
    #[must_use]
    pub fn next_sample(&mut self) -> f32 {
        self.voices
            .iter_mut()
            .filter_map(Option::as_mut)
            .map(|active| active.voice.next_sample())
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::AudioVoiceManager;

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
    fn note_on_adds_voice_and_note_off_removes_it() {
        let mut manager = AudioVoiceManager::new(2, 48_000.0).expect("valid configuration");

        assert!(manager.note_on(10, 440.0, 0.5));
        assert_eq!(manager.voice_count(), 1);

        assert!(manager.note_off(10));
        assert_eq!(manager.voice_count(), 0);
        assert!(!manager.note_off(10));
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
    fn manager_reuses_slots_after_note_off() {
        let mut manager = AudioVoiceManager::new(1, 48_000.0).expect("valid configuration");

        assert!(manager.note_on(1, 440.0, 0.5));
        assert!(manager.note_off(1));
        assert!(manager.note_on(2, 660.0, 0.5));
        assert_eq!(manager.voice_count(), 1);
    }

    #[test]
    fn manager_renders_silence_when_empty() {
        let mut manager = AudioVoiceManager::new(4, 48_000.0).expect("valid configuration");

        assert_eq!(manager.next_sample(), 0.0);
    }
}
