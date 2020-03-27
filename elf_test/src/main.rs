use gimli as _;
use std::fs;
use std::path::PathBuf;
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

struct TypePrinter {
    size: usize,
    alignment: usize,

    // ... how to do this part?
    //
    // NextStep(printer or deeper nested type)
    //
    // printer: Vec<ByteRange, Printer>
}

fn main() -> Result<(), anyhow::Error> {
    let opts = Opts::from_args();
    println!("opts: {:#?}", opts.elf);

    let bytes = fs::read(opts.elf)?;
    let elf = &ElfFile::new(&bytes).map_err(anyhow::Error::msg)?;
    let endian = match elf.header.pt1.data() {
        xmas_elf::header::Data::BigEndian => gimli::RunTimeEndian::Big,
        xmas_elf::header::Data::LittleEndian => gimli::RunTimeEndian::Little,
        _ => panic!("Unknown endian"),
    };

    // Load a section and return as `Cow<[u8]>`.
    let load_section = |id: gimli::SectionId| -> Result<&[u8], gimli::Error> {
        if let Some(section) = elf.find_section_by_name(id.name()) {
            Ok(section.raw_data(&elf))
        } else {
            Ok(&[][..])
        }
    };

    // Load a supplementary section. We don't have a supplementary object file,
    // so always return an empty slice.
    let load_section_sup = |_| Ok(&[][..]);

    // Load all of the sections.
    let dwarf = gimli::Dwarf::load(&load_section, &load_section_sup)?;

    // Borrow a `Cow<[u8]>` to create an `EndianSlice`.
    let borrow_section = |&section| gimli::EndianSlice::new(section, endian);

    // Create `EndianSlice`s for all of the sections.
    let dwarf = dwarf.borrow(borrow_section);

    // Iterate over the compilation units.
    let mut iter = dwarf.units();
    while let Some(header) = iter.next()? {
        println!("Unit at <.debug_info+0x{:x}>", header.offset().0);
        let unit = dwarf.unit(header)?;

        // Iterate over the Debugging Information Entries (DIEs) in the unit.
        let mut depth = 0;
        let mut entries = unit.entries();
        while let Some((delta_depth, entry)) = entries.next_dfs()? {
            depth += delta_depth;
            println!("<depth: {}><{:x}> {}", depth, entry.offset().0, entry.tag());

            // Iterate over the attributes in the DIE.
            let mut attrs = entry.attrs();
            while let Some(attr) = attrs.next()? {
                if attr.name() == gimli::constants::DW_AT_name {
                    if let gimli::read::AttributeValue::DebugStrRef(r) = attr.value() {
                        if let Ok(s) = dwarf.string(r) {
                            if let Ok(s) = s.to_string() {
                                println!("   {}: {}", attr.name(), s);
                            }
                        }
                    }
                } else {
                    println!("   {}: {:x?}", attr.name(), attr.value());
                }
            }
        }
    }

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
                            if name == "TEST" {
                                println!(
                                    "        Found '{}', address = 0x{:8x}, size = {}b",
                                    name,
                                    entry.value(),
                                    entry.size()
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
