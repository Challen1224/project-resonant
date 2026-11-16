//! Preallocated multi-voice audio mixing.

use crate::voice::AudioVoice;

/// Mixes multiple audio voices into a caller-provided output buffer.
///
/// Voice storage is allocated when the mixer is created. Adding voices may
/// allocate if capacity is exceeded, so voice management should happen
/// outside the real-time audio callback. Rendering itself does not allocate.
#[derive(Debug)]
pub struct AudioMixer {
    voices: Vec<AudioVoice>,
}

impl AudioMixer {
    /// Creates a mixer with capacity for the requested number of voices.
    #[must_use]
    pub fn with_capacity(max_voices: usize) -> Self {
        Self {
            voices: Vec::with_capacity(max_voices),
        }
    }

    /// Returns the number of active voices.
    #[must_use]
    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    /// Returns the currently allocated voice capacity.
    #[must_use]
    pub fn voice_capacity(&self) -> usize {
        self.voices.capacity()
    }

    /// Adds a voice if the mixer has available capacity.
    ///
    /// # Errors
    ///
    /// Returns the rejected voice when the mixer is full.
    pub fn add_voice(&mut self, voice: AudioVoice) -> Result<(), AudioVoice> {
        if self.voices.len() == self.voices.capacity() {
            return Err(voice);
        }

        self.voices.push(voice);
        Ok(())
    }

    /// Removes every active voice without releasing allocated storage.
    pub fn clear(&mut self) {
        self.voices.clear();
    }

    /// Renders all voices into the supplied buffer.
    ///
    /// Existing buffer samples are replaced. Samples are summed without
    /// clipping so downstream processing can apply its chosen limiter.
    pub fn render(&mut self, output: &mut [f32]) {
        for sample in output {
            *sample = self.next_sample();
        }
    }

    /// Generates one sample by summing all active voices.
    #[must_use]
    pub fn next_sample(&mut self) -> f32 {
        self.voices.iter_mut().map(AudioVoice::next_sample).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::AudioMixer;
    use crate::voice::AudioVoice;

    fn make_voice(frequency_hz: f32, gain: f32) -> AudioVoice {
        AudioVoice::new(frequency_hz, 48_000.0, gain).expect("valid voice configuration")
    }

    #[test]
    fn mixer_starts_empty_with_requested_capacity() {
        let mixer = AudioMixer::with_capacity(4);

        assert_eq!(mixer.voice_count(), 0);
        assert!(mixer.voice_capacity() >= 4);
    }

    #[test]
    fn mixer_adds_voices_up_to_capacity() {
        let mut mixer = AudioMixer::with_capacity(2);

        assert!(mixer.add_voice(make_voice(440.0, 0.5)).is_ok());
        assert!(mixer.add_voice(make_voice(660.0, 0.5)).is_ok());
        assert_eq!(mixer.voice_count(), 2);

        let rejected = mixer.add_voice(make_voice(880.0, 0.5));
        assert!(rejected.is_err());
        assert_eq!(mixer.voice_count(), 2);
    }

    #[test]
    fn mixer_sums_multiple_voices() {
        let mut mixer = AudioMixer::with_capacity(2);

        mixer
            .add_voice(make_voice(440.0, 0.5))
            .expect("capacity available");
        mixer
            .add_voice(make_voice(660.0, 0.25))
            .expect("capacity available");

        let mut output = [1.0; 8];
        mixer.render(&mut output);

        let mut first = make_voice(440.0, 0.5);
        let mut second = make_voice(660.0, 0.25);

        for sample in &output {
            let expected = first.next_sample() + second.next_sample();
            assert!((*sample - expected).abs() < 0.000_001);
        }
    }

    #[test]
    fn mixer_next_sample_sums_active_voices() {
        let mut mixer = AudioMixer::with_capacity(2);
        mixer
            .add_voice(make_voice(440.0, 0.5))
            .expect("capacity available");
        mixer
            .add_voice(make_voice(660.0, 0.25))
            .expect("capacity available");

        assert_eq!(mixer.next_sample(), 0.0);

        let mut first = make_voice(440.0, 0.5);
        let mut second = make_voice(660.0, 0.25);
        let _ = first.next_sample();
        let _ = second.next_sample();

        let expected = first.next_sample() + second.next_sample();
        let actual = mixer.next_sample();

        assert!((actual - expected).abs() < 0.000_001);
    }

    #[test]
    fn mixer_replaces_existing_output_samples() {
        let mut mixer = AudioMixer::with_capacity(1);
        mixer
            .add_voice(make_voice(440.0, 0.5))
            .expect("capacity available");

        let mut output = [10.0; 4];
        mixer.render(&mut output);

        assert_eq!(output[0], 0.0);
        assert!(output.iter().all(|sample| sample.abs() <= 0.5));
    }

    #[test]
    fn mixer_handles_empty_output_buffers() {
        let mut mixer = AudioMixer::with_capacity(1);
        mixer
            .add_voice(make_voice(440.0, 0.5))
            .expect("capacity available");

        mixer.render(&mut []);

        assert_eq!(mixer.voice_count(), 1);
    }

    #[test]
    fn mixer_clear_preserves_capacity() {
        let mut mixer = AudioMixer::with_capacity(2);
        let capacity = mixer.voice_capacity();

        mixer
            .add_voice(make_voice(440.0, 0.5))
            .expect("capacity available");
        mixer.clear();

        assert_eq!(mixer.voice_count(), 0);
        assert_eq!(mixer.voice_capacity(), capacity);
    }
}
