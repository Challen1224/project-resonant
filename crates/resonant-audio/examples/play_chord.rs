//! Plays a three-note chord through the multi-voice audio mixer.

use std::thread;
use std::time::Duration;

use resonant_audio::device::AudioMixerOutput;
use resonant_audio::mixer::AudioMixer;
use resonant_audio::voice::AudioVoice;

fn main() {
    println!("Project Resonant — Multi-Voice Chord");
    println!("Preparing a C major chord...");

    let mut mixer = AudioMixer::with_capacity(3);

    let notes = [(261.63, 0.035), (329.63, 0.035), (392.00, 0.035)];

    for (frequency_hz, gain) in notes {
        let voice =
            AudioVoice::new(frequency_hz, 48_000.0, gain).expect("valid chord voice configuration");

        mixer
            .add_voice(voice)
            .expect("mixer has capacity for all three chord voices");
    }

    println!("Starting three simultaneous voices...");

    let _output = match AudioMixerOutput::start(mixer) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("Could not start mixer playback: {error}");
            std::process::exit(1);
        }
    };

    println!("Playing C major chord for five seconds.");
    thread::sleep(Duration::from_secs(5));

    println!("Chord demonstration complete.");
}
