//! Demonstrates all four oscillator waveforms through live audio output.

use std::thread;
use std::time::Duration;

use resonant_audio::command::{NoteCommand, note_command_channel};
use resonant_audio::device::AudioCommandOutput;
use resonant_audio::envelope::AdsrParameters;
use resonant_audio::oscillator::Waveform;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Project Resonant — Oscillator Waveform Showcase");

    let envelope =
        AdsrParameters::new(0.02, 0.08, 0.65, 0.2).ok_or("Invalid ADSR configuration")?;

    let (mut sender, receiver) = note_command_channel(16)?;
    let _output = AudioCommandOutput::start_with_envelope(receiver, 4, envelope)
        .map_err(std::io::Error::other)?;

    let waveforms = [
        ("Sine", Waveform::Sine),
        ("Square", Waveform::Square),
        ("Sawtooth", Waveform::Sawtooth),
        ("Triangle", Waveform::Triangle),
    ];

    for (index, (name, waveform)) in waveforms.into_iter().enumerate() {
        let voice_id = index as u64 + 1;

        println!("Playing {name} waveform...");

        sender
            .try_send(NoteCommand::NoteOnWithWaveform {
                voice_id,
                frequency_hz: 440.0,
                gain: 0.18,
                waveform,
            })
            .map_err(|_| std::io::Error::other("Audio command queue is full"))?;

        thread::sleep(Duration::from_millis(900));

        sender
            .try_send(NoteCommand::NoteOff { voice_id })
            .map_err(|_| std::io::Error::other("Audio command queue is full"))?;

        thread::sleep(Duration::from_millis(300));
    }

    println!("Waveform showcase complete.");
    thread::sleep(Duration::from_millis(250));

    Ok(())
}
