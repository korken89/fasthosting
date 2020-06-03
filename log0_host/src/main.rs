use anyhow::Result;
use gimli as _;
use log0_host::{bytes_to_read, fmt, parser::Parser};
use probe_rs::{
    flashing::{download_file_with_options, DownloadOptions, FlashProgress, Format},
    MemoryInterface, Probe, WireProtocol,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use structopt::StructOpt;
use xmas_elf::ElfFile;

#[derive(StructOpt)]
struct Opts {
    #[structopt(name = "FILE", parse(from_os_str))]
    elf: PathBuf,
}

fn main() -> Result<()> {
    let opts = Opts::from_args();
    // println!("opts: {:#?}", opts.elf);

    // Get address of cursors
    let bytes = fs::read(&opts.elf)?;
    let elf = &ElfFile::new(&bytes).map_err(anyhow::Error::msg)?;

    // -------------------------------------------------------------------
    //
    // Setup debug probe
    //
    // -------------------------------------------------------------------

    // Get a list of all available debug probes.
    let probes = Probe::list_all();
    println!("Probes: {:#?}", probes);

    // Use the first probe found.
    let mut probe = probes[0].open()?;
    probe.select_protocol(WireProtocol::Swd)?;
    let speed_khz = probe.set_speed(24_000)?;
    println!("Probe speed: {} kHz", speed_khz);

    // Attach to a chip.
    let mut session = probe.attach("stm32l412cbu")?;

    print!("Spinning up the binary ...");
    download_file_with_options(
        &mut session,
        Path::new(&opts.elf),
        Format::Elf,
        DownloadOptions {
            progress: Some(&FlashProgress::new(|_event| {
                print!(".");
            })),
            keep_unwritten_bytes: false,
        },
    )?;
    let mut core = session.core(0)?;
    core.reset_and_halt()?;

    println!(" Done!");

    std::thread::sleep(std::time::Duration::from_millis(500));

    // -------------------------------------------------------------------
    //
    // Read from MCU
    //
    // -------------------------------------------------------------------

    let fmt::Res {
        map_strings,
        map_types,
        cursor_address,
        buffer_address,
        buffer_size,
    } = fmt::extract_format_and_type_strings(&elf)?;

    // Ctrl-C handling
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    let mut old_target = 0;
    let mut read_buff = vec![0; buffer_size];
    let mut parser = Parser::new();

    core.run()?;

    while running.load(Ordering::SeqCst) {
        let mut buff = [0u32; 2];

        let now = Instant::now();

        core.read_32(cursor_address, &mut buff)?;

        let target = buff[0];
        let host = buff[1];

        if target != old_target {
            old_target = target;

            let br = bytes_to_read(host as usize, target as usize, buffer_size);
            // println!("bytes to read: {}", br);

            let mut read = &mut read_buff[0..br];

            if host + br as u32 > buffer_size as u32 {
                // cursor will overflow
                let pivot = buffer_size - host as usize;
                // println!(
                //     "pivot: {}, reading from {} to {}, 0 to {}",
                //     pivot,
                //     host,
                //     host + pivot as u32,
                //     br - pivot
                // );
                core.read_8(buffer_address + host, &mut read[0..pivot])?;
                core.read_8(buffer_address, &mut read[pivot..br])?;
                core.write_word_32(cursor_address + 4, (br - pivot) as u32)?;
            } else {
                // println!("reading from {} to {}", host, host + br as u32);
                core.read_8(buffer_address + host, &mut read)?;
                core.write_word_32(cursor_address + 4, (host + br as u32) % buffer_size as u32)?;
            }

            let _dur = now.elapsed();

            parser.push(&read);

            while let Some(packet) = parser.try_parse() {
                println!(
                    "String: '{}', Type string: '{}', Buffer: {:x?}",
                    map_strings
                        .get(&packet.string_loc)
                        .unwrap_or(&"String not found in hashmap?!?!?!"),
                    map_types
                        .get(&packet.type_loc)
                        .unwrap_or(&"String not found in hashmap?!?!?!"),
                    packet.buffer
                );

                // println!("packet: {:x?}", p);
            }

            // println!("target: {}, host: {}, len to read: {}", target, host, br,);
            // println!("read buf: {:x?}", read);
            // println!("");
        }
    }

    core.halt()?;

    println!("Exiting ...");

    Ok(())
}
