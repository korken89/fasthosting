use probe_rs::{
    flashing::{download_file_with_options, DownloadOptions, FlashProgress, Format},
    Probe, WireProtocol,
};
use std::path::Path;

fn main() -> Result<(), probe_rs::Error> {
    // Get a list of all available debug probes.
    let probes = Probe::list_all();
    println!("probes: {:#?}", probes);

    // Use the first probe found.
    let mut probe = probes[0].open()?;
    println!("probe: {:#?}", probe);
    probe.select_protocol(WireProtocol::Swd)?;
    probe.set_speed(24_000)?;

    // Attach to a chip.
    let session = probe.attach("stm32l412cbu")?;
    // let session = probe.attach(TargetSelector::Auto)?;
    // println!("session: {:#?}", session);

    // Select a core.
    let mm = session.memory_map();
    println!("memory map: {:#x?}", mm);

    download_file_with_options(
        &session,
        Path::new("../minimal_program/target/thumbv7em-none-eabihf/release/app"),
        Format::Elf,
        DownloadOptions {
            progress: Some(&FlashProgress::new(|event| {
                println!("event: {:#?}", event);
            })),
            keep_unwritten_bytes: false,
        },
    )
    .unwrap();

    let core = session.attach_to_core(0)?;
    // println!("core: {:#?}", core);

    // Halt the attached core.
    // core.halt()?;

    // Read a single 32 bit word.
    let word = core.read_word_32(0x2000_0000)?;
    println!("word: {:x}", word);
    core.write_word_32(0x2000_0000, 0x1234)?;
    let word = core.read_word_32(0x2000_0000)?;
    println!("word: {:x}", word);

    Ok(())
}
