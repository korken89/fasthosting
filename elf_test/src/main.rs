use elf_test::generate_printers;

use std::fs;
use std::path::PathBuf;
use structopt::StructOpt;

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

fn main() -> Result<(), anyhow::Error> {
    let opts = Opts::from_args();
    println!("opts: {:#?}", opts.elf);

    let bytes = fs::read(opts.elf)?;

    let _printers = generate_printers(&bytes)?;

    Ok(())
}
