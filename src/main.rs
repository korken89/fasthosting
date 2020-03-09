use probe_rs::{config::TargetSelector, Probe};

fn main() -> Result<(), probe_rs::Error> {
    // Get a list of all available debug probes.
    let probes = Probe::list_all();
    println!("probes: {:#?}", probes);

    // Use the first probe found.
    let probe = probes[0].open()?;
    println!("probe: {:#?}", probe);

    // Attach to a chip.
    let session = probe.attach(TargetSelector::Auto)?;
    // println!("session: {:#?}", session);

    // Select a core.
    let core = session.attach_to_core(0)?;
    // println!("core: {:#?}", core);

    // Halt the attached core.
    core.halt()?;

    Ok(())
}
