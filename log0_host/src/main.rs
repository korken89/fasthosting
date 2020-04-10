use gimli as _;
use probe_rs::{
    flashing::{download_file_with_options, DownloadOptions, FlashProgress, Format},
    Probe, WireProtocol,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use structopt::StructOpt;
use xmas_elf::{
    sections::{SectionData, SHF_ALLOC},
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
    println!("opts: {:#?}", opts.elf);

    // Get address of cursors
    let bytes = fs::read(&opts.elf)?;
    let elf = &ElfFile::new(&bytes).map_err(anyhow::Error::msg)?;

    let mut cursor_address = None;
    let mut buf_address = None;

    for sect in elf.section_iter() {
        if sect.flags() & SHF_ALLOC != 0 {
            println!("alloc section: {:?}", sect.get_name(elf));
        } else {
            println!("not alloc section: {:?}", sect.get_name(elf));
        }

        if sect.get_name(elf) == Ok(".symtab") {
            if let Ok(symtab) = sect.get_data(elf) {
                if let SectionData::SymbolTable32(entries) = symtab {
                    for entry in entries {
                        if let Ok(name) = entry.get_name(elf) {
                            // println!("names: {}", rustc_demangle::demangle(name).to_string());
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

    // Get a list of all available debug probes.
    let probes = Probe::list_all();
    // println!("probes: {:#?}", probes);

    // Use the first probe found.
    let mut probe = probes[0].open()?;
    // println!("probe: {:#?}", probe);
    probe.select_protocol(WireProtocol::Swd)?;
    let speed_khz = probe.set_speed(24_000)?;
    println!("Probe speed: {} kHz", speed_khz);

    // Attach to a chip.
    let session = probe.attach("stm32l412cbu")?;
    // let session = probe.attach(TargetSelector::Auto)?;
    // println!("session: {:#?}", session);

    // Select a core.
    // let mm = session.memory_map();
    // println!("memory map: {:#x?}", mm);

    download_file_with_options(
        &session,
        Path::new(&opts.elf),
        Format::Elf,
        DownloadOptions {
            progress: Some(&FlashProgress::new(|event| {
                // println!("event: {:#?}", event);
            })),
            keep_unwritten_bytes: false,
        },
    )?;
    //println!("res: {:#?}", res);

    std::thread::sleep(std::time::Duration::from_millis(1000));

    let core = session.attach_to_core(0)?;
    core.reset_and_halt()?;
    core.run()?;
    // println!("core: {:#?}", core);

    // Halt the attached core.
    // core.halt()?;

    // Read a single 32 bit word.
    let mut old_target = 0;
    let buf_size = buf_address.unwrap().1;
    let mut read_buff = vec![0; buf_size];

    loop {
        let mut buff = [0u32; 2];

        let now = Instant::now();

        core.read_32(cursor_address.unwrap(), &mut buff)?;

        let target = buff[0];
        let host = buff[1];

        if target != old_target {
            old_target = target;

            let br = bytes_to_read(host as usize, target as usize, buf_size);

            let mut read = &mut read_buff[0..br];

            if host + br as u32 > buf_size as u32 {
                // cursor will overflow
                let pivot = host.wrapping_add(br as u32).wrapping_sub(buf_size as u32) as usize;
                core.read_8(buf_address.unwrap().0 + host, &mut read[0..pivot])?;
                core.read_8(buf_address.unwrap().0, &mut read[pivot..br])?;
                core.write_word_32(cursor_address.unwrap() + 4, (br - pivot) as u32)?;
            } else {
                core.read_8(buf_address.unwrap().0 + host, &mut read)?;
                core.write_word_32(
                    cursor_address.unwrap() + 4,
                    (host + br as u32) % buf_size as u32,
                )?;
            }
            let dur = now.elapsed();

            println!(
                "target: {}, host: {}, len to read: {}, read time: {} ms",
                target,
                host,
                br,
                dur.as_secs_f64() * 1000.0
            );
            println!("read buf: {:?}", read,);
        }
    }

    Ok(())
}

fn bytes_to_read(host_idx: usize, target_idx: usize, buffer_size: usize) -> usize {
    // let cursor = host_idx;
    let len_to_read = target_idx.wrapping_sub(host_idx).wrapping_add(buffer_size) % buffer_size;

    return len_to_read;

    // if cursor + len_to_read > buffer_size {
    //     let pivot = cursor.wrapping_add(len_to_read).wrapping_sub(buffer_size);
    //     unsafe {
    //         core::ptr::copy_nonoverlapping(
    //             data.as_ptr(),
    //             self.buf.add(cursor.into()),
    //             pivot.into(),
    //         );
    //         core::ptr::copy_nonoverlapping(
    //             data.as_ptr().add(pivot.into()),
    //             self.buf,
    //             (len - pivot).into(),
    //         );
    //     }
    // } else {
    //     // single memcpy
    //     unsafe {
    //         core::ptr::copy_nonoverlapping(data.as_ptr(), self.buf.add(cursor.into()), len.into())
    //     }
    // }
}
