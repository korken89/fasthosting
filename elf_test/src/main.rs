use elf_test::PrinterTree;
use fallible_iterator::FallibleIterator;
use gimli::{
    self, constants,
    read::{AttributeValue, DebuggingInformationEntry, Reader},
    DwAte, Dwarf, EndianSlice, EntriesCursor, RunTimeEndian, Unit,
};
use std::collections::HashMap;
use std::convert::TryInto;
use std::fs;
use std::path::PathBuf;
use structopt::StructOpt;
use xmas_elf::ElfFile;

#[derive(StructOpt)]
struct Opts {
    #[structopt(name = "FILE", parse(from_os_str))]
    elf: PathBuf,
}

//
// What do we want?
// ----------------
//
// A structure for type lookup which when given a buffer can print the type.
//
// Eg:
//
// fn generate_printers(elf: &ElfFile) -> Wrapper(HashMap<TypeString, Printer>) { ... }
//
// where Wrapper.print("app::my_type", &buf) will print the type based on the data in buf
//

pub struct TypePrinters(HashMap<String, PrinterTree>);

impl TypePrinters {
    pub fn print(&self, _type_string: &str, _buffer: &[u8]) {
        todo!()
    }
}

fn generate_printers(elf: &ElfFile) -> Result<TypePrinters, anyhow::Error> {
    let endian = match elf.header.pt1.data() {
        xmas_elf::header::Data::BigEndian => RunTimeEndian::Big,
        xmas_elf::header::Data::LittleEndian => RunTimeEndian::Little,
        _ => panic!("Unknown endian"),
    };

    // Load a section and return as `&[u8]`.
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
    let dwarf = Dwarf::load(&load_section, &load_section_sup)?;

    // Create `EndianSlice`s for all of the sections.
    let dwarf = dwarf.borrow(|&section| EndianSlice::new(section, endian));

    // Iterate over the compilation units.
    let mut dwarf_units = dwarf.units();

    // Namespace tracker
    let mut namespace_tracker = Vec::new();

    // Where printers are stored
    let mut printers: HashMap<String, elf_test::PrinterTree> = HashMap::new();

    while let Some(header) = dwarf_units.next()? {
        println!("Unit at <.debug_info+0x{:x}>", header.offset().0);
        let unit = dwarf.unit(header)?;

        println!("unit type: {}", get_type(&unit));

        // Iterate over the Debugging Information Entries (DIEs) in the unit.
        let mut depth = 0;
        let mut entries = unit.entries();

        println!("Entries: {}", get_type(&entries));

        let entries = &mut entries;

        let mut skip_dfs = 0;

        while let Some((delta_depth, die_entry)) = entries.next_dfs()? {
            depth += delta_depth;

            if skip_dfs > 0 {
                skip_dfs -= 1;
                continue;
            }

            // if delta_depth > 0 {
            //     println!("increase depth...");
            // } else if delta_depth < 0 {
            //     println!("decrease depth...");
            // } else {
            //     println!("same depth...");
            // }
            //..

            println!(
                "<depth: {}><0x{:x}> {}",
                depth,
                die_entry.offset().0,
                die_entry.tag()
            );

            // Namespace tracking
            while depth <= namespace_tracker.len() as isize && namespace_tracker.len() > 0 {
                namespace_tracker.pop();
            }

            // Namespace tracking
            if die_entry.tag() == constants::DW_TAG_namespace {
                let namespace_str = if let AttributeValue::DebugStrRef(r) =
                    die_entry.attrs().next()?.unwrap().value()
                {
                    rustc_demangle::demangle(&dwarf.string(r)?.to_string()?)
                } else {
                    panic!("error")
                };

                namespace_tracker.push(namespace_str.to_string());
            }

            // Check for base type
            if die_entry.tag().is_base_type() {
                // If we are tracking a complex type, add base type to it
                //
                // Type that we want to record has been found!!! Encode it in a printer tree for
                // later use
                println!(
                    ">>>>>>>>> base type, depth = {}, name = {:?}",
                    depth,
                    get_base_type_info(&dwarf, &die_entry)
                );

                if let Ok(Some((name, enc, size))) = get_base_type_info(&dwarf, &die_entry) {
                    printers.insert(
                        name.clone(),
                        elf_test::PrinterTree::new_from_base_type(enc, &name, size),
                    );
                }
            } else if die_entry.tag().is_complex_type() {
                // TODO: Start extraction of complex type here

                // Type that we want to record has been found!!! Encode it in a printer tree for
                // later use
                println!(
                    ">>>>>>>>> complex type - in {}, depth = {}",
                    namespace_tracker.join("::"),
                    depth
                );

                let steps_to_skip = extract_complex_type(
                    &dwarf,
                    &unit,
                    entries.clone(), //&mut entries.clone(), // Look down using a clone
                    &mut printers,
                    &mut namespace_tracker,
                    &mut depth,
                )?;

                skip_dfs = steps_to_skip;
            }

            // Iterate over the attributes in the DIE.
            // let mut attrs = die_entry.attrs();
            // while let Some(attr) = attrs.next()? {
            //     if attr.name() == constants::DW_AT_name {
            //         if let AttributeValue::DebugStrRef(r) = attr.value() {
            //             if let Ok(s) = dwarf.string(r) {
            //                 if let Ok(s) = s.to_string() {
            //                     println!("   {}: {}", attr.name(), s);
            //                 }
            //             }
            //         }
            //     }
            //     // else {
            //     //     println!("   {}: {:x?}", attr.name(), attr.value());
            //     // }
            // }
        }
    }

    println!("Printers: {:#?}", printers);

    println!("i32: {:#?}", printers.get("i32"));

    Ok(TypePrinters(printers))
}

fn extract_complex_type(
    dwarf: &Dwarf<EndianSlice<RunTimeEndian>>,
    unit: &Unit<EndianSlice<RunTimeEndian>, usize>,
    mut entries: EntriesCursor<EndianSlice<RunTimeEndian>>,
    printers: &mut HashMap<String, elf_test::PrinterTree>,
    namespace_tracker: &mut Vec<String>,
    depth: &mut isize,
) -> Result<usize, anyhow::Error> {
    let start_depth = *depth;
    let entries = &mut entries;

    let mut skips = 0;

    if let Some(die_entry) = entries.current() {
        println!("Something here!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!1");
        // Iterate over the attributes in the DIE.
        // let mut attrs = die_entry.attrs();
        // while let Some(attr) = attrs.next()? {
        //     if attr.name() == constants::DW_AT_name {
        //         if let AttributeValue::DebugStrRef(r) = attr.value() {
        //             if let Ok(s) = dwarf.string(r) {
        //                 if let Ok(s) = s.to_string() {
        //                     println!("   {}: {}", attr.name(), s);
        //                 }
        //             }
        //         }
        //     } else {
        //         println!("   {}: {:x?}", attr.name(), attr.value());
        //     }
        // }

        let attrs = die_entry.attrs().collect::<Vec<_>>()?;
        for attr in attrs {
            if attr.name() == constants::DW_AT_name {
                if let AttributeValue::DebugStrRef(r) = attr.value() {
                    if let Ok(s) = dwarf.string(r) {
                        if let Ok(s) = s.to_string() {
                            println!("   {}<0x{:x}>: {}", attr.name(), die_entry.offset().0, s);
                        }
                    }
                }
            } else if attr.name() == constants::DW_AT_type {
                if let AttributeValue::UnitRef(unit_offset) = attr.value() {
                    println!("      DW_AT_type: {:x?}", unit_offset);
                    let mut entry_lookup = unit.entries_at_offset(unit_offset)?;
                    let mut ldepth = 0;

                    while let Some((delta_depth, die_entry)) = entry_lookup.next_dfs()? {
                        ldepth += delta_depth;
                        println!(
                            "        <ldepth: {}><0x{:x}> {}",
                            ldepth,
                            die_entry.offset().0,
                            die_entry.tag()
                        );

                        if die_entry.tag().is_base_type() {
                            // If we are tracking a complex type, add base type to it
                            //
                            // Type that we want to record has been found!!! Encode it in a printer tree for
                            // later use
                            println!(
                                "         >>>>>>>>> base type, ldepth = {}, name = {:?}",
                                ldepth,
                                get_base_type_info(&dwarf, &die_entry)
                            );
                        } else if die_entry.tag().is_complex_type() {
                            // TODO: Start extraction of complex type here

                            // Type that we want to record has been found!!! Encode it in a printer tree for
                            // later use
                            println!(
                                ">>>>>>>>> complex type - in {}, depth = {}",
                                namespace_tracker.join("::"),
                                depth
                            );

                            println!(">>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>> 1",);

                            extract_complex_type(
                                &dwarf,
                                &unit,
                                unit.entries_at_offset(die_entry.offset())?.clone(),
                                printers,
                                namespace_tracker,
                                depth,
                            )?;

                            // unit.entries_at_offset(offset).tag().is_base_type()
                        }

                        if ldepth <= 0 {
                            break;
                        }
                    }

                    // println!("          {:x?}", entry_lookup);
                    // println!("          {:x?}", entry_lookup.next_dfs()?);
                }
            } else {
                println!("   {}: {:x?}", attr.name(), attr.value());
            }
        }
    }

    while let Some((delta_depth, die_entry)) = entries.next_dfs()? {
        *depth += delta_depth;

        if *depth <= start_depth {
            break;
        }

        let attrs = die_entry.attrs().collect::<Vec<_>>()?;
        for attr in attrs {
            if attr.name() == constants::DW_AT_name {
                if let AttributeValue::DebugStrRef(r) = attr.value() {
                    if let Ok(s) = dwarf.string(r) {
                        if let Ok(s) = s.to_string() {
                            println!("   {}<0x{:x}>: {}", attr.name(), die_entry.offset().0, s);
                        }
                    }
                }
            } else if attr.name() == constants::DW_AT_type {
                if let AttributeValue::UnitRef(unit_offset) = attr.value() {
                    println!(
                        "      DW_AT_type<0x{:x}>: {:x?}",
                        die_entry.offset().0,
                        unit_offset
                    );
                    let mut entry_lookup = unit.entries_at_offset(unit_offset)?;
                    let mut ldepth = 0;

                    while let Some((delta_depth, die_entry)) = entry_lookup.next_dfs()? {
                        ldepth += delta_depth;
                        println!(
                            "        <ldepth: {}><0x{:x}> {}",
                            ldepth,
                            die_entry.offset().0,
                            die_entry.tag()
                        );

                        if die_entry.tag().is_base_type() {
                            // If we are tracking a complex type, add base type to it
                            //
                            // Type that we want to record has been found!!! Encode it in a printer tree for
                            // later use
                            println!(
                                "         >>>>>>>>> base type, ldepth = {}, name = {:?}",
                                ldepth,
                                get_base_type_info(&dwarf, &die_entry)
                            );
                        } else if die_entry.tag().is_complex_type() {
                            // TODO: Start extraction of complex type here

                            // Type that we want to record has been found!!! Encode it in a printer tree for
                            // later use
                            println!(
                                ">>>>>>>>> complex type - in {}, depth = {}",
                                namespace_tracker.join("::"),
                                depth
                            );

                            println!(">>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>> 2",);

                            println!("        <0x{:x}> {}", die_entry.offset().0, die_entry.tag());

                            // let mut i = unit.entries_at_offset(die_entry.offset())?;
                            // while let Some((delta_depth, die_entry)) = i.next_dfs()? {
                            //     println!(">>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>> 3",);
                            //     let attrs = die_entry.attrs().collect::<Vec<_>>()?;
                            //     for attr in attrs {
                            //         if attr.name() == constants::DW_AT_name {
                            //             if let AttributeValue::DebugStrRef(r) = attr.value() {
                            //                 if let Ok(s) = dwarf.string(r) {
                            //                     if let Ok(s) = s.to_string() {
                            //                         println!(
                            //                             "   {}<0x{:x}>: {}",
                            //                             attr.name(),
                            //                             die_entry.offset().0,
                            //                             s
                            //                         );
                            //                     }
                            //                 }
                            //             }
                            //         } else {
                            //             println!("   {}: {:x?}", attr.name(), attr.value());
                            //         }
                            //     }
                            // }

                            extract_complex_type(
                                &dwarf,
                                &unit,
                                unit.entries_at_offset(die_entry.offset())?.clone(),
                                printers,
                                namespace_tracker,
                                depth,
                            )?;

                            // unit.entries_at_offset(offset).tag().is_base_type()
                        }

                        if ldepth <= 0 {
                            break;
                        }
                    }

                    // println!("          {:x?}", entry_lookup);
                    // println!("          {:x?}", entry_lookup.next_dfs()?);
                }
            } else {
                println!("   {}: {:x?}", attr.name(), attr.value());
            }
        }

        // println!(
        //     "   <cdepth: {}><0x{:x}> {}",
        //     depth,
        //     die_entry.offset().0,
        //     die_entry.tag()
        // );
    }

    println!(">>>>>>>>> complex type exit");

    Ok(0)
}

fn main() -> Result<(), anyhow::Error> {
    let opts = Opts::from_args();
    println!("opts: {:#?}", opts.elf);

    let bytes = fs::read(opts.elf)?;
    let elf = &ElfFile::new(&bytes).map_err(anyhow::Error::msg)?;

    let printers = generate_printers(&elf)?;

    Ok(())
}

#[allow(dead_code)]
fn get_type<T>(_: &T) -> &'static str {
    core::any::type_name::<T>()
}

fn get_base_type_info<T: Reader>(
    dwarf: &Dwarf<T>,
    die_entry: &DebuggingInformationEntry<T>,
) -> Result<Option<(String, DwAte, usize)>, anyhow::Error> {
    let mut name: Option<String> = None;
    let mut encoding: Option<DwAte> = None;
    let mut size: Option<usize> = None;

    let mut attrs = die_entry.attrs();
    while let Some(attr) = attrs.next()? {
        // Find name
        if attr.name() == constants::DW_AT_name {
            if let AttributeValue::DebugStrRef(r) = attr.value() {
                if let Ok(s) = dwarf.string(r) {
                    if let Ok(s) = s.to_string() {
                        name = Some(s.into());
                    }
                }
            }
        }

        // Find encoding
        if attr.name() == constants::DW_AT_encoding {
            if let AttributeValue::Encoding(enc) = attr.value() {
                encoding = Some(enc);
            }
        }

        // Find size
        if attr.name() == constants::DW_AT_byte_size {
            if let AttributeValue::Udata(s) = attr.value() {
                size = Some(s.try_into()?);
            }
        }
    }

    if let (Some(name), Some(enc), Some(size)) = (name, encoding, size) {
        Ok(Some((name, enc, size)))
    } else {
        Ok(None)
    }
}

trait DwTagExt {
    fn is_base_type(&self) -> bool;
    fn is_complex_type(&self) -> bool;
}

impl DwTagExt for gimli::DwTag {
    fn is_base_type(&self) -> bool {
        use constants as c;
        match *self {
            c::DW_TAG_base_type => true,
            _ => false,
        }
    }

    fn is_complex_type(&self) -> bool {
        use constants as c;
        match *self {
            c::DW_TAG_structure_type
            | c::DW_TAG_enumeration_type
            | c::DW_TAG_union_type
            | c::DW_TAG_array_type
            | c::DW_TAG_reference_type
            | c::DW_TAG_string_type => true,
            _ => false,
        }
    }
}
