#[cfg(any(feature = "gui", test))]
mod bindings;
mod device;
#[cfg(feature = "gui")]
mod gui;
mod protocol;
mod settings;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.is_empty() {
        #[cfg(feature = "gui")]
        return gui::run();
        #[cfg(not(feature = "gui"))]
        return Err("Use --list or --inspect, or build with the gui feature.".into());
    }
    if args == ["--inspect"] {
        for info in device::enumerate()? {
            println!("Trying {}", info.label);
            match device::Mouse::open(&info.path) {
                Ok(mouse) => {
                    let raw = mouse.read_config()?;
                    println!(
                        "Identity {:?}, max rate {} Hz",
                        mouse.identity,
                        mouse.max_rate()
                    );
                    println!("Settings: {:?}", settings::Settings::decode(&raw)?);
                    for command in [4, 18, 29] {
                        println!("Query {command}: {:02x?}", mouse.query(command));
                    }
                    return Ok(());
                }
                Err(error) => println!("Skipped: {error}"),
            }
        }
        return Err("No supported V8 configuration interface responded.".into());
    }
    if args != ["--list"] {
        return Err("Usage: scycocks [--list | --inspect]".into());
    }
    let api = hidapi::HidApi::new()?;
    for info in api
        .device_list()
        .filter(|d| protocol::supported(d.vendor_id(), d.product_id()))
    {
        println!(
            "{:04x}:{:04x} usage={:04x}:{:04x} interface={} product={:?} path={:?}",
            info.vendor_id(),
            info.product_id(),
            info.usage_page(),
            info.usage(),
            info.interface_number(),
            info.product_string(),
            info.path()
        );
        if info.usage_page() >= 0xff00 {
            let device = info.open_device(&api)?;
            let mut descriptor = [0; 4096];
            let size = device.get_report_descriptor(&mut descriptor)?;
            println!("Descriptor: {:02x?}", &descriptor[..size]);
        }
    }
    Ok(())
}
