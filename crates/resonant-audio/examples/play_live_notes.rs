//! Demonstrates live Note On and Note Off commands during audio playback.

use std::thread;
use std::time::Duration;

use resonant_audio::command::{NoteCommand, note_command_channel};
use resonant_audio::device::AudioCommandOutput;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Project Resonant — Live Note Commands");
    println!("Creating a bounded real-time command queue...");

    let (mut sender, receiver) =
        note_command_channel(128).expect("command queue capacity must be nonzero");

    // The output detects the hardware sample rate and prepares its voice
    // manager before starting the audio callback.
    let _output = AudioCommandOutput::start(receiver, 16).map_err(std::io::Error::other)?;

    println!("Audio is running. Sending three Note On commands...");

    let notes = [(1, 261.63, 0.035), (2, 329.63, 0.035), (3, 392.00, 0.035)];

    for (voice_id, frequency_hz, gain) in notes {
        sender
            .try_send(NoteCommand::NoteOn {
                voice_id,
                frequency_hz,
                gain,
            })
            .expect("command queue should have available capacity");
    }

    println!("C major chord playing. Sending Note Off commands in sequence...");
    thread::sleep(Duration::from_secs(2));

    for voice_id in [1, 2, 3] {
        sender
            .try_send(NoteCommand::NoteOff { voice_id })
            .expect("command queue should have available capacity");

        thread::sleep(Duration::from_millis(700));
    }

    println!("All notes released. Sending another Note On...");

    sender
        .try_send(NoteCommand::NoteOn {
            voice_id: 4,
            frequency_hz: 440.0,
            gain: 0.05,
        })
        .expect("command queue should have available capacity");

    thread::sleep(Duration::from_secs(2));

    sender
        .try_send(NoteCommand::NoteOff { voice_id: 4 })
        .expect("command queue should have available capacity");

    println!("Live note command demonstration complete.");
    thread::sleep(Duration::from_millis(250));

    Ok(())
}
