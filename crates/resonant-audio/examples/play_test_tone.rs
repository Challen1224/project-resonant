//! Plays a short sine-wave test tone through the default output device.

use std::thread;
use std::time::Duration;

fn main() {
    println!("Project Resonant — Audio Playback Test");
    println!("Starting a 440 Hz sine wave for three seconds...");

    let _output = match resonant_audio::device::AudioOutput::start_sine(440.0, 0.10) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("Could not start audio playback: {error}");
            std::process::exit(1);
        }
    };

    thread::sleep(Duration::from_secs(3));

    println!("Playback test complete.");
}
