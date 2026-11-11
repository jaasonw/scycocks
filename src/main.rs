// Without the GUI only the read paths are reachable.
#![cfg_attr(not(feature = "gui"), allow(dead_code))]

mod action;
mod driver;
mod family;
#[cfg(feature = "gui")]
mod gui;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() {
        #[cfg(feature = "gui")]
        return gui::run();
        #[cfg(not(feature = "gui"))]
        return Err("Use --list or --inspect, or build with the gui feature.".into());
    }
    let (entries, problems) = driver::load();
    for problem in &problems {
        eprintln!("Skipped device file {problem}");
    }
    let candidates = driver::discover(&entries)?;
    match args.as_slice() {
        [flag] if flag == "--list" => {
            for c in &candidates {
                println!("{}  ({})  {:?}", c.label, entries[c.entry].source, c.path);
            }
        }
        [flag] if flag == "--inspect" => {
            // Reads only: identify, status, profile and settings requests.
            for c in &candidates {
                println!("{}", c.label);
                let result = driver::Device::open(&entries, c).and_then(|device| {
                    let snapshot = device.snapshot()?;
                    println!("  {} · {}", device.def().name, snapshot.status);
                    if let Some(profile) = snapshot.profile {
                        println!("  Profile {}", profile + 1);
                    }
                    for (field, value) in device.def().field.iter().zip(&snapshot.values) {
                        println!("  {}: {value:?}", field.label);
                    }
                    Ok(())
                });
                if let Err(error) = result {
                    println!("  Skipped: {error}");
                }
            }
        }
        _ => return Err("Usage: scycocks [--list | --inspect]".into()),
    }
    Ok(())
}
