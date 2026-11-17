//! Preloaded sample playback primitives.

/// Plays a preloaded mono sample without allocating during rendering.
#[derive(Clone, Copy, Debug)]
pub struct SampleVoice {
    position: u64,
    playback_step: u64,
    gain: f32,
    active: bool,
}

impl SampleVoice {
    const FRACTIONAL_BITS: u32 = 16;
    const FRACTIONAL_MASK: u64 = (1 << Self::FRACTIONAL_BITS) - 1;
    const FRACTIONAL_SCALE: f32 = 65_536.0;

    /// Creates a sample voice with validated sample and output rates.
    ///
    /// The sample data remains owned by the caller. Playback uses linear
    /// interpolation and stops automatically at the end of the sample.
    #[must_use]
    pub fn new(source_sample_rate_hz: u32, output_sample_rate_hz: u32, gain: f32) -> Option<Self> {
        if source_sample_rate_hz == 0
            || output_sample_rate_hz == 0
            || !gain.is_finite()
            || !(0.0..=1.0).contains(&gain)
        {
            return None;
        }

        let fixed_point_step = (u64::from(source_sample_rate_hz) << Self::FRACTIONAL_BITS)
            / u64::from(output_sample_rate_hz);

        Some(Self {
            position: 0,
            playback_step: fixed_point_step.max(1),
            gain,
            active: true,
        })
    }

    /// Returns whether the voice has samples remaining.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Returns the next interpolated sample, or silence after playback ends.
    #[must_use]
    pub fn next_sample(&mut self, samples: &[f32]) -> f32 {
        if !self.active || samples.is_empty() {
            self.active = false;
            return 0.0;
        }

        let Ok(index) = usize::try_from(self.position >> Self::FRACTIONAL_BITS) else {
            self.active = false;
            return 0.0;
        };

        if index >= samples.len() {
            self.active = false;
            return 0.0;
        }

        let next_index = (index + 1).min(samples.len() - 1);
        let fractional_bits = u16::try_from(self.position & Self::FRACTIONAL_MASK).unwrap_or(0);
        let fraction = f32::from(fractional_bits) / Self::FRACTIONAL_SCALE;

        let current = samples[index];
        let next = samples[next_index];

        let output = (current + (next - current) * fraction) * self.gain;

        self.position = self.position.saturating_add(self.playback_step);

        let playback_finished = match usize::try_from(self.position >> Self::FRACTIONAL_BITS) {
            Ok(next_index) => next_index >= samples.len(),
            Err(_) => true,
        };

        if playback_finished {
            self.active = false;
        }

        output
    }
}

/// An immutable mono sample stored at its original sample rate.
#[derive(Clone, Debug)]
pub struct SampleData {
    sample_rate_hz: u32,
    samples: Vec<f32>,
}

impl SampleData {
    /// Returns the source sample rate.
    #[must_use]
    pub fn sample_rate_hz(&self) -> u32 {
        self.sample_rate_hz
    }

    /// Returns the preloaded mono sample frames.
    #[must_use]
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }
}

/// Owns preloaded sample data for real-time playback.
///
/// Populate the bank before starting the audio stream. Once moved into the
/// renderer, sample data is only read during audio rendering.
#[derive(Clone, Debug, Default)]
pub struct SampleBank {
    samples: Vec<SampleData>,
}

impl SampleBank {
    /// Creates an empty sample bank.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a mono sample and returns its stable bank-local identifier.
    ///
    /// Empty samples, zero sample rates, non-finite audio data, and sample
    /// counts exceeding the identifier range are rejected.
    pub fn add_sample(&mut self, sample_rate_hz: u32, samples: Vec<f32>) -> Option<u32> {
        if sample_rate_hz == 0
            || samples.is_empty()
            || samples.iter().any(|sample| !sample.is_finite())
        {
            return None;
        }

        let sample_id = u32::try_from(self.samples.len()).ok()?;

        self.samples.push(SampleData {
            sample_rate_hz,
            samples,
        });

        Some(sample_id)
    }

    /// Returns a preloaded sample by its bank-local identifier.
    #[must_use]
    pub fn get(&self, sample_id: u32) -> Option<&SampleData> {
        self.samples.get(usize::try_from(sample_id).ok()?)
    }

    /// Returns the number of preloaded samples.
    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Returns whether the bank contains no samples.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::SampleVoice;

    #[test]
    fn sample_bank_stores_and_retrieves_preloaded_audio() {
        use super::SampleBank;

        let mut bank = SampleBank::new();

        assert!(bank.is_empty());
        assert_eq!(bank.len(), 0);

        let sample_id = bank
            .add_sample(44_100, vec![0.1, -0.2, 0.3])
            .expect("valid sample");

        let sample = bank.get(sample_id).expect("sample exists");

        assert_eq!(sample.sample_rate_hz(), 44_100);
        assert_eq!(sample.samples(), &[0.1, -0.2, 0.3]);
        assert_eq!(bank.len(), 1);
        assert!(!bank.is_empty());
    }

    #[test]
    fn sample_bank_rejects_invalid_audio_data() {
        use super::SampleBank;

        let mut bank = SampleBank::new();

        assert!(bank.add_sample(0, vec![0.1]).is_none());
        assert!(bank.add_sample(48_000, vec![]).is_none());
        assert!(bank.add_sample(48_000, vec![f32::NAN]).is_none());
        assert!(bank.add_sample(48_000, vec![f32::INFINITY]).is_none());

        assert!(bank.is_empty());
    }

    #[test]
    fn sample_bank_returns_none_for_unknown_identifiers() {
        use super::SampleBank;

        let bank = SampleBank::new();

        assert!(bank.get(0).is_none());
        assert!(bank.get(u32::MAX).is_none());
    }

    #[test]
    fn sample_voice_plays_preloaded_samples_in_order() {
        let mut voice =
            SampleVoice::new(48_000, 48_000, 0.5).expect("valid sample voice configuration");

        let samples = [0.2, 0.4, 0.6];

        assert_eq!(voice.next_sample(&samples), 0.1);
        assert_eq!(voice.next_sample(&samples), 0.2);
        assert_eq!(voice.next_sample(&samples), 0.3);
        assert!(!voice.is_active());
        assert_eq!(voice.next_sample(&samples), 0.0);
    }

    #[test]
    fn sample_voice_interpolates_when_sample_rates_differ() {
        let mut voice =
            SampleVoice::new(24_000, 48_000, 1.0).expect("valid sample voice configuration");

        let samples = [0.0, 1.0, 0.0];

        assert_eq!(voice.next_sample(&samples), 0.0);
        assert_eq!(voice.next_sample(&samples), 0.5);
        assert_eq!(voice.next_sample(&samples), 1.0);
        assert_eq!(voice.next_sample(&samples), 0.5);
    }

    #[test]
    fn sample_voice_rejects_invalid_configuration() {
        assert!(SampleVoice::new(0, 48_000, 0.5).is_none());
        assert!(SampleVoice::new(48_000, 0, 0.5).is_none());
        assert!(SampleVoice::new(48_000, 48_000, -0.1).is_none());
        assert!(SampleVoice::new(48_000, 48_000, 1.1).is_none());
        assert!(SampleVoice::new(48_000, 48_000, f32::NAN).is_none());
    }

    #[test]
    fn sample_voice_handles_empty_samples() {
        let mut voice =
            SampleVoice::new(48_000, 48_000, 0.5).expect("valid sample voice configuration");

        assert_eq!(voice.next_sample(&[]), 0.0);
        assert!(!voice.is_active());
    }

    #[test]
    fn sample_voice_supports_downsampling() {
        let mut voice =
            SampleVoice::new(96_000, 48_000, 1.0).expect("valid sample voice configuration");

        let samples = [0.1, 0.2, 0.3, 0.4, 0.5];

        assert_eq!(voice.next_sample(&samples), 0.1);
        assert_eq!(voice.next_sample(&samples), 0.3);
        assert_eq!(voice.next_sample(&samples), 0.5);
        assert!(!voice.is_active());
    }
}
