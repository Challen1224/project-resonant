//! Demonstrates live frequency and amplitude changes during audio playback.

use std::thread;
use std::time::Duration;

use resonant_audio::device::AudioOutput;

fn main() {
    println!("Project Resonant — Live Audio Controls");
    println!("Starting a quiet 440 Hz tone...");

    let output = match AudioOutput::start_sine(440.0, 0.08) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("Could not start audio playback: {error}");
            std::process::exit(1);
        }
    };

    thread::sleep(Duration::from_secs(2));

    println!("Changing frequency to 660 Hz...");
    assert!(output.set_frequency_hz(660.0));
    thread::sleep(Duration::from_secs(2));

    println!("Changing frequency to 330 Hz...");
    assert!(output.set_frequency_hz(330.0));
    thread::sleep(Duration::from_secs(2));

    println!("Reducing amplitude...");
    assert!(output.set_amplitude(0.025));
    thread::sleep(Duration::from_secs(2));

    println!("Restoring amplitude...");
    assert!(output.set_amplitude(0.08));
    thread::sleep(Duration::from_secs(2));

    println!("Live control demonstration complete.");
}
