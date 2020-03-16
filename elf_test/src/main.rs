use std::fs;
use std::path::PathBuf;
use structopt::StructOpt;
use xmas_elf::{sections::SectionData, symbol_table::Entry, ElfFile};

#[derive(StructOpt)]
struct Opts {
    // #[structopt(short, long, parse(try_from_str = parse_hex))]
    // vendor: u16,

    // #[structopt(short, long, parse(try_from_str = parse_hex))]
    // product: u16,

    // #[structopt(long)]
    // verify: bool,
    #[structopt(long, parse(from_os_str))]
    elf: PathBuf,
}

fn main() -> Result<(), anyhow::Error> {
    let opts = Opts::from_args();
    println!("opts: {:#?}", opts.elf);

    let bytes = fs::read(opts.elf)?;
    let elf = &ElfFile::new(&bytes).map_err(anyhow::Error::msg)?;

    for sect in elf.section_iter() {
        if sect.get_name(elf) == Ok(".symtab") {
            if let Ok(symtab) = sect.get_data(elf) {
                if let SectionData::SymbolTable32(entries) = symtab {
                    for entry in entries {
                        if let Ok(name) = entry.get_name(elf) {
                            if name == "TEST" {
                                println!(
                                    "Found '{}', address = 0x{:8x}, size = {}b",
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
