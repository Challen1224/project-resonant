//! Sample-generating oscillator primitives.

/// Available oscillator waveform shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Waveform {
    /// Smooth sinusoidal waveform.
    Sine,
    /// Two-level waveform with an abrupt transition.
    Square,
    /// Rising ramp waveform with an abrupt wraparound.
    Sawtooth,
    /// Linear rising and falling waveform.
    Triangle,
}

/// A phase-continuous oscillator supporting multiple waveform shapes.
#[derive(Clone, Copy, Debug)]
pub struct Oscillator {
    phase: f32,
    phase_increment: f32,
    sample_rate_hz: f32,
    amplitude: f32,
    waveform: Waveform,
}

impl Oscillator {
    /// Creates a sine-wave oscillator.
    #[must_use]
    pub fn new(frequency_hz: f32, sample_rate_hz: f32, amplitude: f32) -> Option<Self> {
        Self::new_with_waveform(frequency_hz, sample_rate_hz, amplitude, Waveform::Sine)
    }

    /// Creates an oscillator with a selected waveform.
    #[must_use]
    pub fn new_with_waveform(
        frequency_hz: f32,
        sample_rate_hz: f32,
        amplitude: f32,
        waveform: Waveform,
    ) -> Option<Self> {
        if !frequency_hz.is_finite() || !sample_rate_hz.is_finite() || !amplitude.is_finite() {
            return None;
        }

        if sample_rate_hz <= 0.0
            || frequency_hz < 0.0
            || frequency_hz > sample_rate_hz * 0.5
            || !(0.0..=1.0).contains(&amplitude)
        {
            return None;
        }

        Some(Self {
            phase: 0.0,
            phase_increment: std::f32::consts::TAU * frequency_hz / sample_rate_hz,
            sample_rate_hz,
            amplitude,
            waveform,
        })
    }

    /// Changes waveform without resetting oscillator phase.
    pub fn set_waveform(&mut self, waveform: Waveform) {
        self.waveform = waveform;
    }

    /// Returns the currently selected waveform.
    #[must_use]
    pub fn waveform(&self) -> Waveform {
        self.waveform
    }

    /// Updates oscillator frequency while preserving phase.
    pub fn set_frequency_hz(&mut self, frequency_hz: f32) -> bool {
        if !frequency_hz.is_finite()
            || frequency_hz < 0.0
            || frequency_hz > self.sample_rate_hz * 0.5
        {
            return false;
        }

        self.phase_increment = std::f32::consts::TAU * frequency_hz / self.sample_rate_hz;
        true
    }

    /// Updates output amplitude.
    pub fn set_amplitude(&mut self, amplitude: f32) -> bool {
        if !amplitude.is_finite() || !(0.0..=1.0).contains(&amplitude) {
            return false;
        }

        self.amplitude = amplitude;
        true
    }

    /// Generates the next oscillator sample.
    pub fn next_sample(&mut self) -> f32 {
        use std::f32::consts::{PI, TAU};

        let normalized_phase = self.phase / TAU;

        let raw_sample = match self.waveform {
            Waveform::Sine => self.phase.sin(),
            Waveform::Square => {
                if self.phase < PI {
                    1.0
                } else {
                    -1.0
                }
            }
            Waveform::Sawtooth => 2.0 * normalized_phase - 1.0,
            Waveform::Triangle => 1.0 - 4.0 * (normalized_phase - 0.5).abs(),
        };

        let sample = raw_sample * self.amplitude;

        self.phase += self.phase_increment;
        if self.phase >= TAU {
            self.phase -= TAU;
        }

        sample
    }
}

/// Backward-compatible name for the default sine oscillator.
pub type SineOscillator = Oscillator;

#[cfg(test)]
mod tests {
    use super::{Oscillator, SineOscillator, Waveform};
    use std::f32::consts::TAU;

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
    fn oscillator_frequency_can_change_without_resetting_phase() {
        let mut oscillator =
            SineOscillator::new(440.0, 48_000.0, 0.2).expect("valid oscillator configuration");

        let _ = oscillator.next_sample();
        let phase_before_update = oscillator.phase;

        assert!(oscillator.set_frequency_hz(880.0));
        assert_eq!(oscillator.phase, phase_before_update);
        assert_eq!(oscillator.phase_increment, TAU * 880.0 / 48_000.0);
    }

    #[test]
    fn oscillator_rejects_invalid_frequency_updates() {
        let mut oscillator =
            SineOscillator::new(440.0, 48_000.0, 0.2).expect("valid oscillator configuration");
        let original_increment = oscillator.phase_increment;

        assert!(!oscillator.set_frequency_hz(f32::NAN));
        assert!(!oscillator.set_frequency_hz(f32::INFINITY));
        assert!(!oscillator.set_frequency_hz(-1.0));
        assert!(!oscillator.set_frequency_hz(24_001.0));
        assert_eq!(oscillator.phase_increment, original_increment);
    }

    #[test]
    fn oscillator_amplitude_can_change() {
        let mut oscillator =
            SineOscillator::new(440.0, 48_000.0, 0.2).expect("valid oscillator configuration");

        assert!(oscillator.set_amplitude(0.7));
        assert_eq!(oscillator.amplitude, 0.7);
    }

    #[test]
    fn oscillator_rejects_invalid_amplitude_updates() {
        let mut oscillator =
            SineOscillator::new(440.0, 48_000.0, 0.2).expect("valid oscillator configuration");

        assert!(!oscillator.set_amplitude(f32::NAN));
        assert!(!oscillator.set_amplitude(f32::INFINITY));
        assert!(!oscillator.set_amplitude(-0.1));
        assert!(!oscillator.set_amplitude(1.1));
        assert_eq!(oscillator.amplitude, 0.2);
    }

    #[test]
    fn oscillator_generates_a_positive_sample_after_start() {
        let mut oscillator =
            SineOscillator::new(440.0, 48_000.0, 0.2).expect("valid oscillator configuration");

        let _ = oscillator.next_sample();
        assert!(oscillator.next_sample() > 0.0);
    }

    #[test]
    fn square_wave_generates_expected_samples() {
        let mut oscillator =
            Oscillator::new_with_waveform(1.0, 4.0, 1.0, Waveform::Square).unwrap();

        assert_eq!(oscillator.next_sample(), 1.0);
        assert_eq!(oscillator.next_sample(), 1.0);
        assert_eq!(oscillator.next_sample(), -1.0);
        assert_eq!(oscillator.next_sample(), -1.0);
        assert_eq!(oscillator.next_sample(), 1.0);
    }

    #[test]
    fn sawtooth_wave_generates_expected_samples() {
        let mut oscillator =
            Oscillator::new_with_waveform(1.0, 4.0, 1.0, Waveform::Sawtooth).unwrap();

        let expected = [-1.0, -0.5, 0.0, 0.5];

        for sample in expected {
            assert!((oscillator.next_sample() - sample).abs() < 1e-6);
        }
    }

    #[test]
    fn triangle_wave_generates_expected_samples() {
        let mut oscillator =
            Oscillator::new_with_waveform(1.0, 4.0, 1.0, Waveform::Triangle).unwrap();

        let expected = [-1.0, 0.0, 1.0, 0.0];

        for sample in expected {
            assert!((oscillator.next_sample() - sample).abs() < 1e-6);
        }
    }

    #[test]
    fn waveform_change_preserves_phase() {
        let mut oscillator = Oscillator::new(1.0, 4.0, 1.0).unwrap();

        oscillator.next_sample();
        oscillator.set_waveform(Waveform::Square);

        assert_eq!(oscillator.waveform(), Waveform::Square);
        assert_eq!(oscillator.next_sample(), 1.0);
    }

    #[test]
    fn default_constructor_selects_sine_wave() {
        let mut oscillator = Oscillator::new(1.0, 4.0, 1.0).unwrap();

        assert_eq!(oscillator.waveform(), Waveform::Sine);
        assert!((oscillator.next_sample()).abs() < 1e-6);
        assert!((oscillator.next_sample() - 1.0).abs() < 1e-6);
    }
}
