//! Plays a note through each stage of a configurable ADSR envelope.

use std::thread;
use std::time::Duration;

use resonant_audio::command::{NoteCommand, note_command_channel};
use resonant_audio::device::AudioCommandOutput;
use resonant_audio::envelope::AdsrParameters;

fn main() {
    println!("Project Resonant — ADSR Envelope Demonstration");

    let envelope = AdsrParameters::new(0.8, 0.6, 0.3, 1.5)
        .expect("the demonstration envelope parameters are valid");
    let (mut sender, receiver) =
        note_command_channel(16).expect("the demonstration command queue capacity is valid");

    let _output = match AudioCommandOutput::start_with_envelope(receiver, 8, envelope) {
        Ok(output) => output,
        Err(error) => {
            eprintln!("Could not start ADSR playback: {error}");
            std::process::exit(1);
        }
    };

    println!("Attack: 0.8 seconds — listen as the note rises...");
    sender
        .try_send(NoteCommand::NoteOn {
            voice_id: 1,
            frequency_hz: 440.0,
            gain: 0.25,
        })
        .expect("the command queue has room for Note On");

    thread::sleep(Duration::from_millis(1_500));
    println!("Decay complete — sustaining at 30% level...");

    thread::sleep(Duration::from_secs(2));
    println!("Release: 1.5 seconds — listen as the note fades...");
    sender
        .try_send(NoteCommand::NoteOff { voice_id: 1 })
        .expect("the command queue has room for Note Off");

    thread::sleep(Duration::from_secs(2));
    println!("ADSR demonstration complete.");
}
