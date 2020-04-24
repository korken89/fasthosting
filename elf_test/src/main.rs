use gimli as _;
use std::fs;
use std::ops::Range;
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

    // Create `EndianSlice`s for all of the sections.
    let dwarf = dwarf.borrow(|&section| gimli::EndianSlice::new(section, endian));

    // Iterate over the compilation units.
    let mut iter = dwarf.units();
    let mut namespace = Vec::new();
    while let Some(header) = iter.next()? {
        println!("Unit at <.debug_info+0x{:x}>", header.offset().0);
        let unit = dwarf.unit(header)?;

        // TODO:
        //
        // Pass 1: Extract base types
        // Pass 2: Extract complex types that resolve into trees with base types as leafs

        // Iterate over the Debugging Information Entries (DIEs) in the unit.
        let mut depth = 0;
        let mut entries = unit.entries();
        while let Some((delta_depth, entry)) = entries.next_dfs()? {
            depth += delta_depth;
            println!("<depth: {}><{:x}> {}", depth, entry.offset().0, entry.tag());

            while depth <= namespace.len() as isize && namespace.len() > 0 {
                namespace.pop();
            }

            if entry.tag() == gimli::constants::DW_TAG_namespace {
                let namespace_str = if let gimli::read::AttributeValue::DebugStrRef(r) =
                    entry.attrs().next()?.unwrap().value()
                {
                    rustc_demangle::demangle(dwarf.string(r).unwrap().to_string().unwrap())
                } else {
                    panic!("error")
                };

                namespace.push(namespace_str.to_string());
            }

            if entry.tag().is_base_type() {
                // If we are tracking a complex type, add base type to it
                //
                // Type that we want to record has been found!!! Encode it in a printer tree for
                // later use
                println!(">>>>>>>>> base type, depth = {}", depth);
            } else if entry.tag().is_complex_type() {
                // Type that we want to record has been found!!! Encode it in a printer tree for
                // later use
                println!(
                    ">>>>>>>>> complex type - in {}, depth = {}",
                    namespace.join("::"),
                    depth
                );
            }

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

    // for sect in elf.section_iter() {
    //     if sect.flags() & SHF_ALLOC != 0 {
    //         println!("alloc section: {:?}", sect.get_name(elf));
    //     } else {
    //         println!("not alloc section: {:?}", sect.get_name(elf));
    //     }

    //     if sect.get_name(elf) == Ok(".symtab") {
    //         if let Ok(symtab) = sect.get_data(elf) {
    //             if let SectionData::SymbolTable32(entries) = symtab {
    //                 for entry in entries {
    //                     if let Ok(name) = entry.get_name(elf) {
    //                         // println!("names: {}", rustc_demangle::demangle(name).to_string());
    //                         if name == "LOG0_CURSORS" {
    //                             println!(
    //                                 "        Found '{}', address = 0x{:8x}, size = {}b",
    //                                 name,
    //                                 entry.value(),
    //                                 entry.size()
    //                             );
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //     }
    // }

    Ok(())
}

trait DwTagExt {
    fn is_base_type(&self) -> bool;
    fn is_complex_type(&self) -> bool;
}

impl DwTagExt for gimli::DwTag {
    fn is_base_type(&self) -> bool {
        use gimli::constants as c;
        match *self {
            c::DW_TAG_base_type => true,
            _ => false,
        }
    }

    fn is_complex_type(&self) -> bool {
        use gimli::constants as c;
        match *self {
            c::DW_TAG_structure_type
            | c::DW_TAG_union_type
            | c::DW_TAG_array_type
            | c::DW_TAG_reference_type
            | c::DW_TAG_string_type => true,
            _ => false,
        }
    }
}
