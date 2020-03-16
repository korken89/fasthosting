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
    #[structopt(long, parse(from_os_str))]
    elf: PathBuf,
}

fn main() -> Result<(), anyhow::Error> {
    let opts = Opts::from_args();
    println!("opts: {:#?}", opts.elf);

    let bytes = fs::read(opts.elf)?;
    let elf = &ElfFile::new(&bytes).map_err(anyhow::Error::msg)?;

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
                            println!("names: {}", rustc_demangle::demangle(name).to_string());
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
