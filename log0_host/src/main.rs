use gimli as _;
use log0_host::{bytes_to_read, parser::Parser};
use probe_rs::{
    flashing::{download_file_with_options, DownloadOptions, FlashProgress, Format},
    Probe, WireProtocol,
};
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use structopt::StructOpt;
use xmas_elf::{
    sections::{SectionData, SHN_LORESERVE},
    symbol_table::Entry,
    ElfFile,
};

#[derive(StructOpt)]
struct Opts {
    #[structopt(name = "FILE", parse(from_os_str))]
    elf: PathBuf,
}

fn main() -> Result<(), anyhow::Error> {
    let opts = Opts::from_args();
    // println!("opts: {:#?}", opts.elf);

    // Get address of cursors
    let bytes = fs::read(&opts.elf)?;
    let elf = &ElfFile::new(&bytes).map_err(anyhow::Error::msg)?;

    let mut cursor_address = None;
    let mut buf_address = None;

    let sections = get_sections(elf);

    // println!("sections: {:#?}", sections);

    // -------------------------------------------------------------------
    //
    // Extract formating strings and type strings
    //
    // -------------------------------------------------------------------

    let mut map_strings: HashMap<usize, &str> = HashMap::new();
    let mut map_types: HashMap<usize, &str> = HashMap::new();

    for sect in elf.section_iter() {
        // if sect.flags() & SHF_ALLOC != 0 {
        //     println!(
        //         "alloc section: {:?}, address: {:x}, size: {}",
        //         sect.get_name(elf),
        //         sect.address(),
        //         sect.size()
        //     );
        // } else {
        //     println!(
        //         "not alloc section: {:?}, address: {:x}, size: {}",
        //         sect.get_name(elf),
        //         sect.address(),
        //         sect.size()
        //     );
        // }

        if sect.get_name(elf) == Ok(".symtab") {
            if let Ok(symtab) = sect.get_data(elf) {
                if let SectionData::SymbolTable32(entries) = symtab {
                    for entry in entries {
                        if let Ok(name) = entry.get_name(elf) {
                            // println!(
                            //     "names: {}, addr: {:x}, size: {}, shndx: {}",
                            //     rustc_demangle::demangle(name).to_string(),
                            //     entry.value(),
                            //     entry.size(),
                            //     entry.shndx(),
                            // );

                            if entry.shndx() < SHN_LORESERVE {
                                if let Ok(s) = elf.section_header(entry.shndx()) {
                                    let ev = entry.value() as usize;
                                    let es = entry.size() as usize;
                                    if let Ok(".fasthosting") = s.get_name(elf) {
                                        let cs = sections
                                            .iter()
                                            .find(|v| &v.name == &".fasthosting")
                                            .unwrap();

                                        // offset for byte array
                                        let ev_off = ev - cs.address as usize;

                                        if let Ok(s) =
                                            std::str::from_utf8(&cs.bytes[ev_off..ev_off + es])
                                        {
                                            map_strings.insert(ev, s);
                                        }
                                    }

                                    if let Ok(".rodata") = s.get_name(elf) {
                                        let cs = sections
                                            .iter()
                                            .find(|v| &v.name == &".rodata")
                                            .unwrap();

                                        // offset for byte array
                                        let ev_off = ev - cs.address as usize;

                                        if let Ok(s) =
                                            std::str::from_utf8(&cs.bytes[ev_off..ev_off + es])
                                        {
                                            map_types.insert(ev, s);
                                        }
                                    }
                                }
                            }

                            if name == "LOG0_CURSORS" {
                                println!(
                                    "        Found '{}', address = 0x{:8x}, size = {}b",
                                    name,
                                    entry.value(),
                                    entry.size()
                                );

                                cursor_address = Some(entry.value() as u32);
                            }

                            if name == "LOG0_BUFFER" {
                                println!(
                                    "        Found '{}', address = 0x{:8x}, size = {}b",
                                    name,
                                    entry.value(),
                                    entry.size()
                                );

                                buf_address = Some((entry.value() as u32, entry.size() as usize));
                            }
                        }
                    }
                }
            }
        }
    }

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
    let session = probe.attach("stm32l412cbu")?;

    print!("Loading binary ");
    download_file_with_options(
        &session,
        Path::new(&opts.elf),
        Format::Elf,
        DownloadOptions {
            progress: Some(&FlashProgress::new(|_event| {
                print!(".");
            })),
            keep_unwritten_bytes: false,
        },
    )?;
    println!(" Done!");

    std::thread::sleep(std::time::Duration::from_millis(1000));

    let core = session.attach_to_core(0)?;
    core.reset_and_halt()?;
    core.run()?;
    // println!("core: {:#?}", core);

    // Halt the attached core.
    // core.halt()?;

    // -------------------------------------------------------------------
    //
    // Read from MCU
    //
    // -------------------------------------------------------------------

    // Ctrl-C handling
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");

    // Read a single 32 bit word.
    let cursor_address = cursor_address.expect("cursor address not found");
    let buf_address = buf_address.expect("cursor address not found");
    let mut old_target = 0;
    let buf_size = buf_address.1;
    let mut read_buff = vec![0; buf_size];
    let mut parser = Parser::new();

    while running.load(Ordering::SeqCst) {
        let mut buff = [0u32; 2];

        let now = Instant::now();

        core.read_32(cursor_address, &mut buff)?;

        let target = buff[0];
        let host = buff[1];

        if target != old_target {
            old_target = target;

            let br = bytes_to_read(host as usize, target as usize, buf_size);
            // println!("bytes to read: {}", br);

            let mut read = &mut read_buff[0..br];

            if host + br as u32 > buf_size as u32 {
                // cursor will overflow
                let pivot = buf_size - host as usize;
                // println!(
                //     "pivot: {}, reading from {} to {}, 0 to {}",
                //     pivot,
                //     host,
                //     host + pivot as u32,
                //     br - pivot
                // );
                core.read_8(buf_address.0 + host, &mut read[0..pivot])?;
                core.read_8(buf_address.0, &mut read[pivot..br])?;
                core.write_word_32(cursor_address + 4, (br - pivot) as u32)?;
            } else {
                // println!("reading from {} to {}", host, host + br as u32);
                core.read_8(buf_address.0 + host, &mut read)?;
                core.write_word_32(cursor_address + 4, (host + br as u32) % buf_size as u32)?;
            }

            let _dur = now.elapsed();

            parser.push(&read);

            while let Some(p) = parser.try_parse() {
                println!(
                    "String: '{}', Type string: '{}', Buffer: {:x?}",
                    map_strings
                        .get(&p.string_loc)
                        .unwrap_or(&"String not found in hashmap?!?!?!"),
                    map_types
                        .get(&p.type_loc)
                        .unwrap_or(&"String not found in hashmap?!?!?!"),
                    p.buffer
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

struct Section<'a> {
    address: u32,
    bytes: &'a [u8],
    name: &'a str,
}

impl<'a> fmt::Debug for Section<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Section")
            .field("name", &self.name)
            .field("address", &self.address)
            .field("bytes", &format_args!("_"))
            .finish()
    }
}

fn get_sections<'a>(elf: &'a ElfFile) -> Vec<Section<'a>> {
    let mut sections = Vec::new();

    for sect in elf.section_iter() {
        let size = sect.size();
        if size != 0 {
            if let Ok(name) = sect.get_name(elf) {
                let address = sect.address();
                let max = u64::from(u32::max_value());
                if address > max || address + size > max {
                    continue;
                }

                let align = std::mem::size_of::<u32>() as u64;
                if address % align != 0 {
                    continue;
                }

                sections.push(Section {
                    address: address as u32,
                    bytes: sect.raw_data(elf),
                    name,
                })
            }
        }
    }

    sections
}
