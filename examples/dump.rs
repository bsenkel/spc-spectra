//! Prints a summary of an SPC file.
//!
//! ```text
//! cargo run --example dump -- spectrum.spc
//! ```
//!
//! This doubles as the tool for checking the parser against real instrument
//! files: run it and see whether the numbers match what the acquisition
//! software reported.

use spc_spectra::Spc;

fn main() -> std::process::ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: dump <file.spc>");
        return std::process::ExitCode::FAILURE;
    };

    let spc = match Spc::from_path(&path) {
        Ok(spc) => spc,
        Err(e) => {
            eprintln!("{path}: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let h = &spc.header;
    println!("File          {path}");
    println!("Version       {:#04X}", h.fversn);
    println!("Technique     {}", h.fexper);
    println!("Source        {}", show(&h.fsource));
    println!("Resolution    {}", show(&h.fres));
    println!(
        "Date          {}",
        h.date.map_or("-".into(), |d| d.to_string())
    );
    println!("Comment       {}", show(&h.fcmnt));
    println!("Flags         {:#010b}", h.ftflgs.0);
    println!("Points        {}", spc.y().len());
    println!("Scans         {}", spc.subfiles[0].subheader.subscan);
    println!(
        "x axis        {} .. {}  [{}]",
        h.ffirst,
        h.flast,
        spc.x_label()
    );
    println!("y axis        [{}]", spc.y_label());

    let sub = &spc.subfiles[0];
    println!("\n{:>14}  {:>14}", "x", "y");
    let n = sub.len();
    for (i, (x, y)) in sub.points().enumerate() {
        if i < 5 || i >= n.saturating_sub(5) {
            println!("{x:>14.4}  {y:>14.6}");
        } else if i == 5 {
            println!("{:>14}  {:>14}", "...", "...");
        }
    }

    match &spc.log {
        Some(log) => {
            println!(
                "\nLog block at byte {} ({} binary bytes)",
                h.flogoff,
                log.binary.len()
            );
            for (k, v) in log.entries() {
                println!("  {k} = {v}");
            }
            if log.entries().next().is_none() && !log.text.is_empty() {
                println!("  (not key=value; raw text follows)\n{}", log.text);
            }
        }
        None => println!("\nNo log block."),
    }

    std::process::ExitCode::SUCCESS
}

fn show(s: &str) -> &str {
    if s.is_empty() { "-" } else { s }
}
