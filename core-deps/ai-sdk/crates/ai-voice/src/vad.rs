//! Voice activity detection: a real energy-based VAD (RMS threshold with
//! hangover), extended with per-frame decisions for a whole chunk and
//! deterministic noise-floor adaptation.
//!
//! Adaptation: the effective threshold is
//! `max(fixed_floor, rolling_median(recent non-speech RMS) * noise_factor)`.
//! Only frames classified as non-speech update the rolling window, so speech
//! energy never inflates the noise floor. No randomness anywhere: identical
//! input always produces identical decisions.

use std::collections::VecDeque;
use std::time::Duration;

use crate::Audio;

/// The VAD's decision for a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VadDecision {
    Speech,
    Silence,
    /// Speech just ended (post-speech hangover frame).
    SpeechEnd,
}

/// VAD configuration.
#[derive(Debug, Clone)]
pub struct VadConfig {
    /// Fixed minimum RMS threshold; also the floor for the adaptive rule.
    pub rms_threshold: f32,
    /// Frames of silence before declaring `SpeechEnd`.
    pub hangover_frames: usize,
    /// Frame size in samples.
    pub frame_samples: usize,
    /// Enable adaptive noise-floor tracking (recommended).
    pub adaptive_noise_floor: bool,
    /// Multiplier applied to the rolling median of recent non-speech RMS.
    /// Must be >= 1 so the adaptive part can never fall below observed noise.
    pub noise_factor: f32,
    /// How many recent non-speech frames form the rolling window.
    pub noise_window_frames: usize,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            rms_threshold: 500.0,
            hangover_frames: 8,
            frame_samples: 320, // 20 ms @ 16 kHz
            adaptive_noise_floor: true,
            noise_factor: 2.5,
            noise_window_frames: 64,
        }
    }
}

/// Energy-based voice activity detector with optional noise-floor adaptation.
#[derive(Debug, Clone)]
pub struct VoiceActivityDetector {
    config: VadConfig,
    silence_run: usize,
    in_speech: bool,
    /// Rolling window of recent NON-speech frame RMS values (oldest first).
    noise_window: VecDeque<f32>,
}

impl VoiceActivityDetector {
    pub fn new(config: VadConfig) -> Self {
        let window = config.noise_window_frames.max(1);
        Self {
            config: VadConfig {
                noise_window_frames: window,
                ..config
            },
            silence_run: 0,
            in_speech: false,
            noise_window: VecDeque::with_capacity(window),
        }
    }

    /// Effective threshold for a frame given the current noise estimate:
    /// `max(fixed_floor, rolling_median * k)` when adaptation is enabled.
    fn current_threshold(&self) -> f32 {
        if !self.config.adaptive_noise_floor || self.noise_window.is_empty() {
            return self.config.rms_threshold;
        }
        let mut levels: Vec<f32> = self.noise_window.iter().copied().collect();
        levels.sort_by(|a, b| a.total_cmp(b));
        let mid = levels.len() / 2;
        let median = if levels.len() % 2 == 1 {
            levels[mid]
        } else {
            (levels[mid - 1] + levels[mid]) / 2.0
        };
        (median * self.config.noise_factor.max(1.0)).max(self.config.rms_threshold)
    }

    fn observe_noise(&mut self, rms: f32) {
        if self.noise_window.len() >= self.config.noise_window_frames {
            self.noise_window.pop_front();
        }
        self.noise_window.push_back(rms);
    }

    /// Processes one audio chunk (may span several frames); returns the
    /// decision of the LAST frame. Equivalent to
    /// [`Self::process_frames`] `.last().unwrap_or(Silence)`.
    pub fn process(&mut self, audio: &Audio) -> VadDecision {
        self.process_frames(audio)
            .into_iter()
            .next_back()
            .unwrap_or(VadDecision::Silence)
    }

    /// Processes one audio chunk and returns the decision of EVERY frame in
    /// it, in order (additive API; `process` semantics are unchanged).
    ///
    /// Hangover applies across chunk boundaries: a chunk that ends mid-
    /// hangover reports `Speech` frames and the next chunk continues the
    /// countdown toward `SpeechEnd`.
    pub fn process_frames(&mut self, audio: &Audio) -> Vec<VadDecision> {
        let mut decisions = Vec::with_capacity(audio.samples.len() / self.config.frame_samples);
        for frame in audio.samples.chunks(self.config.frame_samples) {
            let rms = rms(frame);
            let threshold = self.current_threshold();
            if rms >= threshold {
                self.in_speech = true;
                self.silence_run = 0;
                decisions.push(VadDecision::Speech);
            } else if self.in_speech {
                // Hangover frames are still speech-contaminated: they must not
                // be fed into the noise-floor window.
                self.silence_run += 1;
                if self.silence_run >= self.config.hangover_frames {
                    self.in_speech = false;
                    self.silence_run = 0;
                    decisions.push(VadDecision::SpeechEnd);
                } else {
                    decisions.push(VadDecision::Speech);
                }
            } else {
                self.observe_noise(rms);
                decisions.push(VadDecision::Silence);
            }
        }
        decisions
    }

    /// The threshold the NEXT frame will be compared against (exposed for
    /// diagnostics and tests).
    pub fn effective_threshold(&self) -> f32 {
        self.current_threshold()
    }

    pub fn in_speech(&self) -> bool {
        self.in_speech
    }

    /// Resets the detector state (speech state AND learned noise floor).
    pub fn reset(&mut self) {
        self.in_speech = false;
        self.silence_run = 0;
        self.noise_window.clear();
    }

    /// Convenience for callers that need the configured frame duration.
    pub fn frame_duration(&self) -> Duration {
        Duration::from_millis((self.config.frame_samples as u64 * 1000) / 16_000)
    }
}

fn rms(samples: &[i16]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples.iter().map(|s| (*s as f64) * (*s as f64)).sum();
    (sum / samples.len() as f64).sqrt() as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Constant-RMS sine-free tone chunk (deterministic, seedless).
    fn level_chunk(level: i16, frames: usize) -> Audio {
        let samples = vec![level; 320 * frames];
        Audio::from_samples(samples, 16_000)
    }

    #[test]
    fn rms_matches_expected_values() {
        assert_eq!(rms(&[]), 0.0);
        assert_eq!(rms(&[0i16, 0, 0]), 0.0);
        // Constant 1000 → RMS 1000.
        assert!((rms(&[1000i16; 4]) - 1000.0).abs() < 0.001);
    }

    #[test]
    fn hangover_delays_speech_end() {
        let mut detector = VoiceActivityDetector::new(VadConfig {
            hangover_frames: 3,
            ..Default::default()
        });
        let speech = Audio::from_samples(vec![2000i16; 320], 16_000);
        let silence = Audio::from_samples(vec![0i16; 320], 16_000);

        assert_eq!(detector.process(&speech), VadDecision::Speech);
        // Two silence frames: still "speech" (hangover).
        assert_eq!(detector.process(&silence), VadDecision::Speech);
        assert_eq!(detector.process(&silence), VadDecision::Speech);
        // Third silence frame: end.
        assert_eq!(detector.process(&silence), VadDecision::SpeechEnd);
        assert!(!detector.in_speech());
    }

    #[test]
    fn reset_clears_state() {
        let mut detector = VoiceActivityDetector::new(VadConfig::default());
        detector.process(&Audio::from_samples(vec![2000i16; 320], 16_000));
        assert!(detector.in_speech());
        detector.reset();
        assert!(!detector.in_speech());
    }

    #[test]
    fn process_reports_last_frame_but_process_frames_exposes_every_frame() {
        let mut detector = VoiceActivityDetector::new(VadConfig {
            hangover_frames: 2,
            ..Default::default()
        });
        // One chunk: [quiet, quiet, LOUD, LOUD, quiet(hangover), quiet(end)].
        let mut samples = Vec::new();
        samples.extend(std::iter::repeat_n(10i16, 320 * 2));
        samples.extend(std::iter::repeat_n(3000i16, 320 * 2));
        samples.extend(std::iter::repeat_n(10i16, 320 * 2));
        let audio = Audio::from_samples(samples, 16_000);

        let per_frame = detector.process_frames(&audio);
        assert_eq!(
            per_frame,
            vec![
                VadDecision::Silence,
                VadDecision::Silence,
                VadDecision::Speech,
                VadDecision::Speech,
                VadDecision::Speech,    // hangover frame 1
                VadDecision::SpeechEnd, // hangover exhausted
            ]
        );
        // Legacy API returns the same final verdict.
        let mut detector2 = VoiceActivityDetector::new(VadConfig {
            hangover_frames: 2,
            ..Default::default()
        });
        assert_eq!(detector2.process(&audio), VadDecision::SpeechEnd);
    }

    #[test]
    fn adaptive_threshold_ignores_false_triggers_on_ramped_noise() {
        // Background noise whose RMS ramps from 60 up to 160 across the clip.
        let mut noise_samples = Vec::new();
        for step in 0..40usize {
            let level = 60 + step * (100 / 39); // 60..=160
            noise_samples.extend(std::iter::repeat_n(level as i16, 320));
        }
        let noise = Audio::from_samples(noise_samples, 16_000);
        let loud_burst = level_chunk(3000, 4);

        // Fixed threshold at 100: the top of the ramp falsely reads as speech.
        let mut fixed = VoiceActivityDetector::new(VadConfig {
            adaptive_noise_floor: false,
            rms_threshold: 100.0,
            hangover_frames: 2,
            ..Default::default()
        });
        let mut detector = VoiceActivityDetector::new(VadConfig {
            adaptive_noise_floor: true,
            rms_threshold: 100.0,
            noise_factor: 2.5,
            hangover_frames: 2,
            noise_window_frames: 16,
            ..Default::default()
        });

        let fixed_decisions = fixed.process_frames(&noise);
        let adaptive_decisions = detector.process_frames(&noise);
        let fixed_false_triggers = fixed_decisions
            .iter()
            .filter(|d| **d == VadDecision::Speech)
            .count();
        let adaptive_false_triggers = adaptive_decisions
            .iter()
            .filter(|d| **d == VadDecision::Speech)
            .count();

        assert!(
            fixed_false_triggers >= 5,
            "ramp tail should cross the fixed 100 threshold (got {fixed_false_triggers})"
        );
        assert_eq!(
            adaptive_false_triggers, 0,
            "adaptation must suppress ramped-noise false triggers"
        );

        // Both detectors still catch a genuinely loud burst afterwards.
        assert_eq!(detector.process(&loud_burst), VadDecision::Speech);
        assert_eq!(fixed.process(&loud_burst), VadDecision::Speech);
    }

    #[test]
    fn hangover_is_respected_under_adaptation_and_noise_window_stays_clean() {
        let mut detector = VoiceActivityDetector::new(VadConfig {
            adaptive_noise_floor: true,
            rms_threshold: 300.0,
            noise_factor: 2.5,
            hangover_frames: 3,
            noise_window_frames: 8,
            ..Default::default()
        });
        // Quiet pre-roll teaches the noise floor (~10 RMS).
        let _ = detector.process(&level_chunk(10, 6));

        let speech = level_chunk(4000, 2);
        let silence = level_chunk(10, 1);
        assert_eq!(detector.process(&speech), VadDecision::Speech);
        assert_eq!(detector.process(&silence), VadDecision::Speech); // hangover 1/3
        assert_eq!(detector.process(&silence), VadDecision::Speech); // 2/3
        assert_eq!(detector.process(&silence), VadDecision::SpeechEnd); // 3/3

        // Next quiet frame is plain Silence again (post-hangover), proving the
        // hangover counter restarted cleanly.
        assert_eq!(detector.process(&silence), VadDecision::Silence);
    }

    #[test]
    fn adaptation_is_deterministic_seedless() {
        let run = || {
            let mut d = VoiceActivityDetector::new(VadConfig {
                adaptive_noise_floor: true,
                rms_threshold: 100.0,
                noise_factor: 2.0,
                noise_window_frames: 8,
                hangover_frames: 2,
                ..Default::default()
            });
            let mut out = Vec::new();
            for step in 0..12usize {
                out.push(d.process(&level_chunk((20 + step * 10) as i16, 1)));
            }
            out
        };
        assert_eq!(run(), run(), "same input must give identical decisions");
    }
}
