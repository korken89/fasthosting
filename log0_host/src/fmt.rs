use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::fmt;
use xmas_elf::{
    sections::{SectionData, SHN_LORESERVE},
    symbol_table::Entry,
    ElfFile,
};

pub struct Res<'a> {
    pub map_strings: HashMap<usize, &'a str>,
    pub map_types: HashMap<usize, &'a str>,
    pub cursor_address: u32,
    pub buffer_address: u32,
    pub buffer_size: usize,
}

pub fn extract_format_and_type_strings<'a>(elf: &'a ElfFile) -> Result<Res<'a>> {
    let mut cursor_address = None;
    let mut buf_address = None;

    let sections = get_sections(elf);

    // println!("sections: {:#?}", sections);

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
                                // println!(
                                //     "        Found '{}', address = 0x{:8x}, size = {}b",
                                //     name,
                                //     entry.value(),
                                //     entry.size()
                                // );

                                cursor_address = Some(entry.value() as u32);
                            }

                            if name == "LOG0_BUFFER" {
                                // println!(
                                //     "        Found '{}', address = 0x{:8x}, size = {}b",
                                //     name,
                                //     entry.value(),
                                //     entry.size()
                                // );

                                buf_address = Some((entry.value() as u32, entry.size() as usize));
                            }
                        }
                    }
                }
            }
        }
    }

    if cursor_address.is_none() {
        return Err(anyhow!("Missing cursor address"));
    }

    if buf_address.is_none() {
        return Err(anyhow!("Missing buffer address"));
    }

    Ok(Res {
        map_strings,
        map_types,
        cursor_address: cursor_address.unwrap(),
        buffer_address: buf_address.unwrap().0,
        buffer_size: buf_address.unwrap().1,
    })
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
