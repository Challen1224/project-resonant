//! Sample-generating oscillator primitives.

use std::f32::consts::TAU;

/// A sine-wave oscillator that generates one sample at a time.
///
/// The oscillator performs no allocation and is suitable for use inside
/// a real-time audio callback.
#[derive(Clone, Copy, Debug)]
pub struct SineOscillator {
    phase: f32,
    phase_increment: f32,
    amplitude: f32,
}

impl SineOscillator {
    /// Creates an oscillator with the requested frequency, sample rate,
    /// and amplitude.
    ///
    /// Returns `None` if the configuration is invalid or the frequency
    /// exceeds the Nyquist frequency.
    #[must_use]
    pub fn new(frequency_hz: f32, sample_rate_hz: f32, amplitude: f32) -> Option<Self> {
        if !frequency_hz.is_finite()
            || !sample_rate_hz.is_finite()
            || !amplitude.is_finite()
            || sample_rate_hz <= 0.0
            || frequency_hz < 0.0
            || frequency_hz > sample_rate_hz / 2.0
            || !(0.0..=1.0).contains(&amplitude)
        {
            return None;
        }

        Some(Self {
            phase: 0.0,
            phase_increment: TAU * frequency_hz / sample_rate_hz,
            amplitude,
        })
    }

    /// Generates the next audio sample.
    #[must_use]
    pub fn next_sample(&mut self) -> f32 {
        let sample = self.phase.sin() * self.amplitude;

        self.phase += self.phase_increment;

        if self.phase >= TAU {
            self.phase -= TAU;
        }

        sample
    }
}

#[cfg(test)]
mod tests {
    use super::SineOscillator;

    #[test]
    fn oscillator_starts_at_zero() {
        let mut oscillator =
            SineOscillator::new(440.0, 48_000.0, 0.2).expect("valid oscillator configuration");

        assert_eq!(oscillator.next_sample(), 0.0);
    }

    #[test]
    fn oscillator_generates_samples_within_amplitude() {
        let mut oscillator =
            SineOscillator::new(440.0, 48_000.0, 0.2).expect("valid oscillator configuration");

        for _ in 0..48_000 {
            let sample = oscillator.next_sample();
            assert!((-0.2..=0.2).contains(&sample));
        }
    }

    #[test]
    fn oscillator_rejects_invalid_configurations() {
        assert!(SineOscillator::new(440.0, 0.0, 0.2).is_none());
        assert!(SineOscillator::new(-1.0, 48_000.0, 0.2).is_none());
        assert!(SineOscillator::new(24_001.0, 48_000.0, 0.2).is_none());
        assert!(SineOscillator::new(440.0, 48_000.0, 1.5).is_none());
    }

    #[test]
    fn oscillator_generates_a_positive_sample_after_start() {
        let mut oscillator =
            SineOscillator::new(440.0, 48_000.0, 0.2).expect("valid oscillator configuration");

        let _ = oscillator.next_sample();
        assert!(oscillator.next_sample() > 0.0);
    }
}
