//! Lists available audio output devices.

fn main() {
    println!("Project Resonant — Audio Output Devices");
    println!("=======================================");

    match resonant_audio::device::enumerate_output_devices() {
        Ok(devices) if devices.is_empty() => {
            println!("No audio output devices were found.");
        }
        Ok(devices) => {
            for device in devices {
                let marker = if device.is_default { " [DEFAULT]" } else { "" };

                println!("{}{}", device.name, marker);
                println!("  Sample rate: {} Hz", device.sample_rate_hz);
                println!("  Channels:    {}", device.channels);
                println!("  Format:      {}", device.sample_format);
                println!();
            }
        }
        Err(error) => {
            eprintln!("Audio device discovery failed: {error}");
            std::process::exit(1);
        }
    }
}
