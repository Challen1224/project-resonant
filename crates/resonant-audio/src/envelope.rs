//! Allocation-free ADSR amplitude envelopes.

/// Configuration for an attack, decay, sustain, and release envelope.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdsrParameters {
    attack_seconds: f32,
    decay_seconds: f32,
    sustain_level: f32,
    release_seconds: f32,
}

impl AdsrParameters {
    /// Creates validated ADSR parameters.
    ///
    /// Times must be finite and non-negative. Sustain must be within
    /// `0.0..=1.0`.
    #[must_use]
    pub fn new(
        attack_seconds: f32,
        decay_seconds: f32,
        sustain_level: f32,
        release_seconds: f32,
    ) -> Option<Self> {
        if !attack_seconds.is_finite()
            || attack_seconds < 0.0
            || !decay_seconds.is_finite()
            || decay_seconds < 0.0
            || !sustain_level.is_finite()
            || !(0.0..=1.0).contains(&sustain_level)
            || !release_seconds.is_finite()
            || release_seconds < 0.0
        {
            return None;
        }

        Some(Self {
            attack_seconds,
            decay_seconds,
            sustain_level,
            release_seconds,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EnvelopeStage {
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}

/// A linear ADSR envelope whose processing performs no allocation.
///
/// Construct the envelope before audio rendering begins. Its durations are
/// converted to sample counts using the supplied sample rate.
#[derive(Clone, Copy, Debug)]
pub struct AdsrEnvelope {
    parameters: AdsrParameters,
    attack_samples: f32,
    decay_samples: f32,
    release_samples: f32,
    stage: EnvelopeStage,
    level: f32,
    stage_start_level: f32,
    stage_target_level: f32,
    stage_duration_samples: f32,
    stage_elapsed_samples: f32,
}

impl AdsrEnvelope {
    /// Creates an idle envelope using the requested sample rate.
    ///
    /// Returns `None` for an invalid sample rate or if a duration cannot be
    /// represented as a finite sample count.
    #[must_use]
    pub fn new(parameters: AdsrParameters, sample_rate_hz: f32) -> Option<Self> {
        if !sample_rate_hz.is_finite() || sample_rate_hz <= 0.0 {
            return None;
        }

        let attack_samples = parameters.attack_seconds * sample_rate_hz;
        let decay_samples = parameters.decay_seconds * sample_rate_hz;
        let release_samples = parameters.release_seconds * sample_rate_hz;

        if !attack_samples.is_finite() || !decay_samples.is_finite() || !release_samples.is_finite()
        {
            return None;
        }

        Some(Self {
            parameters,
            attack_samples,
            decay_samples,
            release_samples,
            stage: EnvelopeStage::Idle,
            level: 0.0,
            stage_start_level: 0.0,
            stage_target_level: 0.0,
            stage_duration_samples: 0.0,
            stage_elapsed_samples: 0.0,
        })
    }

    /// Starts the envelope from zero and enters its attack stage.
    pub fn note_on(&mut self) {
        self.level = 0.0;
        self.begin_stage(EnvelopeStage::Attack, 0.0, 1.0, self.attack_samples);
        self.finish_zero_duration_stages();
    }

    /// Begins the release stage from the envelope's current level.
    pub fn note_off(&mut self) {
        if matches!(self.stage, EnvelopeStage::Idle | EnvelopeStage::Release) {
            return;
        }

        self.begin_stage(
            EnvelopeStage::Release,
            self.level,
            0.0,
            self.release_samples,
        );
        self.finish_zero_duration_stages();
    }

    /// Returns the current envelope level.
    #[must_use]
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Returns whether the envelope is currently active.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.stage != EnvelopeStage::Idle
    }

    /// Returns whether the envelope is currently in its release stage.
    #[must_use]
    pub fn is_releasing(&self) -> bool {
        self.stage == EnvelopeStage::Release
    }

    /// Generates the next envelope level.
    #[must_use]
    pub fn next_sample(&mut self) -> f32 {
        match self.stage {
            EnvelopeStage::Idle | EnvelopeStage::Sustain => self.level,
            EnvelopeStage::Attack | EnvelopeStage::Decay | EnvelopeStage::Release => {
                self.stage_elapsed_samples += 1.0;

                let progress = (self.stage_elapsed_samples / self.stage_duration_samples).min(1.0);

                self.level = self.stage_start_level
                    + (self.stage_target_level - self.stage_start_level) * progress;

                if self.stage_elapsed_samples >= self.stage_duration_samples {
                    self.level = self.stage_target_level;
                    self.finish_current_stage();
                    self.finish_zero_duration_stages();
                }

                self.level
            }
        }
    }

    fn begin_stage(
        &mut self,
        stage: EnvelopeStage,
        start_level: f32,
        target_level: f32,
        duration_samples: f32,
    ) {
        self.stage = stage;
        self.level = start_level;
        self.stage_start_level = start_level;
        self.stage_target_level = target_level;
        self.stage_duration_samples = duration_samples;
        self.stage_elapsed_samples = 0.0;
    }

    fn finish_current_stage(&mut self) {
        match self.stage {
            EnvelopeStage::Attack => self.begin_stage(
                EnvelopeStage::Decay,
                1.0,
                self.parameters.sustain_level,
                self.decay_samples,
            ),
            EnvelopeStage::Decay => {
                self.stage = EnvelopeStage::Sustain;
                self.level = self.parameters.sustain_level;
            }
            EnvelopeStage::Release => {
                self.stage = EnvelopeStage::Idle;
                self.level = 0.0;
            }
            EnvelopeStage::Idle | EnvelopeStage::Sustain => {}
        }
    }

    fn finish_zero_duration_stages(&mut self) {
        while matches!(
            self.stage,
            EnvelopeStage::Attack | EnvelopeStage::Decay | EnvelopeStage::Release
        ) && self.stage_duration_samples == 0.0
        {
            self.level = self.stage_target_level;
            self.finish_current_stage();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AdsrEnvelope, AdsrParameters};

    fn parameters() -> AdsrParameters {
        AdsrParameters::new(0.002, 0.002, 0.5, 0.002).expect("valid ADSR parameters")
    }

    #[test]
    fn parameters_reject_invalid_values() {
        assert!(AdsrParameters::new(-1.0, 0.1, 0.5, 0.1).is_none());
        assert!(AdsrParameters::new(f32::NAN, 0.1, 0.5, 0.1).is_none());
        assert!(AdsrParameters::new(0.1, -1.0, 0.5, 0.1).is_none());
        assert!(AdsrParameters::new(0.1, 0.1, -0.1, 0.1).is_none());
        assert!(AdsrParameters::new(0.1, 0.1, 1.1, 0.1).is_none());
        assert!(AdsrParameters::new(0.1, 0.1, 0.5, f32::INFINITY).is_none());
    }

    #[test]
    fn envelope_rejects_invalid_sample_rates() {
        let parameters = parameters();

        assert!(AdsrEnvelope::new(parameters, 0.0).is_none());
        assert!(AdsrEnvelope::new(parameters, -1.0).is_none());
        assert!(AdsrEnvelope::new(parameters, f32::NAN).is_none());
    }

    #[test]
    fn envelope_starts_idle_at_zero() {
        let envelope =
            AdsrEnvelope::new(parameters(), 1_000.0).expect("valid envelope configuration");

        assert_eq!(envelope.level(), 0.0);
        assert!(!envelope.is_active());
    }

    #[test]
    fn attack_rises_linearly_to_full_level() {
        let mut envelope =
            AdsrEnvelope::new(parameters(), 1_000.0).expect("valid envelope configuration");

        envelope.note_on();

        assert_eq!(envelope.next_sample(), 0.5);
        assert_eq!(envelope.next_sample(), 1.0);
    }

    #[test]
    fn decay_reaches_the_configured_sustain_level() {
        let mut envelope =
            AdsrEnvelope::new(parameters(), 1_000.0).expect("valid envelope configuration");

        envelope.note_on();

        for _ in 0..4 {
            let _ = envelope.next_sample();
        }

        assert_eq!(envelope.level(), 0.5);
        assert_eq!(envelope.next_sample(), 0.5);
    }

    #[test]
    fn release_fades_to_zero_and_becomes_inactive() {
        let mut envelope =
            AdsrEnvelope::new(parameters(), 1_000.0).expect("valid envelope configuration");

        envelope.note_on();

        for _ in 0..4 {
            let _ = envelope.next_sample();
        }

        envelope.note_off();

        assert_eq!(envelope.next_sample(), 0.25);
        assert_eq!(envelope.next_sample(), 0.0);
        assert!(!envelope.is_active());
    }

    #[test]
    fn repeated_note_off_does_not_restart_release() {
        let mut envelope =
            AdsrEnvelope::new(parameters(), 1_000.0).expect("valid envelope configuration");

        envelope.note_on();

        for _ in 0..4 {
            let _ = envelope.next_sample();
        }

        envelope.note_off();
        assert_eq!(envelope.next_sample(), 0.25);

        envelope.note_off();

        assert_eq!(envelope.next_sample(), 0.0);
        assert!(!envelope.is_active());
    }

    #[test]
    fn zero_duration_stages_are_skipped_safely() {
        let instant_parameters = AdsrParameters::new(0.0, 0.0, 0.4, 0.0).expect("valid parameters");
        let mut envelope = AdsrEnvelope::new(instant_parameters, 48_000.0).expect("valid envelope");

        envelope.note_on();

        assert_eq!(envelope.level(), 0.4);
        assert!(envelope.is_active());

        envelope.note_off();

        assert_eq!(envelope.level(), 0.0);
        assert!(!envelope.is_active());
    }

    #[test]
    fn note_on_restarts_the_envelope() {
        let mut envelope =
            AdsrEnvelope::new(parameters(), 1_000.0).expect("valid envelope configuration");

        envelope.note_on();
        let _ = envelope.next_sample();
        envelope.note_on();

        assert_eq!(envelope.level(), 0.0);
        assert_eq!(envelope.next_sample(), 0.5);
    }
}
