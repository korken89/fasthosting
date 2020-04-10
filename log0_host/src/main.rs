use gimli as _;
use probe_rs::{
    flashing::{download_file_with_options, DownloadOptions, FlashProgress, Format},
    Probe, WireProtocol,
};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
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
    println!("opts: {:#?}", opts.elf);

    // Get address of cursors
    let bytes = fs::read(&opts.elf)?;
    let elf = &ElfFile::new(&bytes).map_err(anyhow::Error::msg)?;

    let mut cursor_address = None;
    let mut buf_address = None;

    let sections = get_sections(elf);
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
                                    if let Ok(".crapsection") = s.get_name(elf) {
                                        let cs = sections
                                            .iter()
                                            .find(|v| &v.name == &".crapsection")
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

    // Get a list of all available debug probes.
    let probes = Probe::list_all();

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

    // Read a single 32 bit word.
    let mut old_target = 0;
    let buf_size = buf_address.unwrap().1;
    let mut read_buff = vec![0; buf_size];
    let mut parser = Parser::new();

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
            let _dur = now.elapsed();

            parser.push(&read);

            while let Some(p) = parser.try_parse() {
                println!(
                    "String: '{}', Type string: '{}', Buffer: {:?}",
                    map_strings
                        .get(&p.string_loc)
                        .unwrap_or(&"String not found in hashmap?!?!?!"),
                    map_types
                        .get(&p.type_loc)
                        .unwrap_or(&"String not found in hashmap?!?!?!"),
                    p.buffer
                );
            }

            // println!(
            //     "target: {}, host: {}, len to read: {}, read time: {:.2} ms",
            //     target,
            //     host,
            //     br,
            //     dur.as_secs_f64() * 1000.0
            // );
            // println!("read buf: {:x?}", read);
        }
    }

    Ok(())
}

fn bytes_to_read(host_idx: usize, target_idx: usize, buffer_size: usize) -> usize {
    target_idx.wrapping_sub(host_idx).wrapping_add(buffer_size) % buffer_size
}

struct Packet {
    string_loc: usize,
    type_loc: usize,
    buffer: Vec<u8>,
}

struct Parser {
    buf: VecDeque<u8>,
    wait_for_size: Option<usize>,
}

impl Parser {
    pub fn new() -> Self {
        Parser {
            buf: VecDeque::with_capacity(10 * 1024 * 1024),
            wait_for_size: None,
        }
    }

    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend(data.iter());
    }

    pub fn try_parse(&mut self) -> Option<Packet> {
        loop {
            if self.wait_for_size == None {
                if self.buf.len() >= 2 {
                    let mut header = [0; 2];
                    header[0] = self.buf.pop_front().unwrap();
                    header[1] = self.buf.pop_front().unwrap();
                    let size = u16::from_le_bytes(header) as usize;
                    self.wait_for_size = Some(size);
                } else {
                    break;
                }
            } else {
                if self.buf.len() >= self.wait_for_size.unwrap() {
                    let mut sym = [0; 4];
                    let mut typ = [0; 4];

                    // Print string location
                    for (i, b) in self.buf.drain(..4).enumerate() {
                        sym[i] = b;
                    }
                    let sym = u32::from_le_bytes(sym);

                    // Type string location
                    for (i, b) in self.buf.drain(..4).enumerate() {
                        typ[i] = b;
                    }
                    let typ = u32::from_le_bytes(typ);

                    // Buffer
                    let buf = self
                        .buf
                        .drain(..self.wait_for_size.unwrap() - 8)
                        .collect::<Vec<_>>();

                    self.wait_for_size = None;

                    return Some(Packet {
                        string_loc: sym as usize,
                        type_loc: typ as usize,
                        buffer: buf,
                    });
                } else {
                    break;
                }
            }
        }

        None
    }
}

struct Section<'a> {
    address: u32,
    bytes: &'a [u8],
    name: &'a str,
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
                if address % align != 0 || size % align != 0 {
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
