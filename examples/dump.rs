//! Prints a summary of an SPC file.
//!
//! ```text
//! cargo run --example dump -- spectrum.spc
//! cargo run --example dump -- series.spc --sub 7
//! ```
//!
//! This doubles as the tool for checking the parser against real instrument
//! files: run it and see whether the numbers match what the acquisition
//! software reported.

use spc_spectra::Spc;

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = args.first().cloned() else {
        eprintln!("usage: dump <file.spc> [--sub N]");
        return std::process::ExitCode::FAILURE;
    };
    // Which spectrum to tabulate, counted from 1 as the file numbers them.
    let wanted = match args.iter().position(|a| a == "--sub") {
        None => 1,
        Some(at) => match args.get(at + 1).and_then(|n| n.parse::<usize>().ok()) {
            Some(n) if n >= 1 => n,
            _ => {
                eprintln!("--sub takes a subfile number, counting from 1");
                return std::process::ExitCode::FAILURE;
            }
        },
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
    println!("Points        {}", spc.subfiles[0].len());
    println!("Subfiles      {}", spc.subfiles.len());
    println!("Scans         {}", spc.subfiles[0].subheader.subscan);
    println!(
        "x axis        {} .. {}  [{}]",
        h.ffirst,
        h.flast,
        spc.x_label()
    );
    println!("y axis        [{}]", spc.y_label());

    let Some(sub) = spc.subfiles.get(wanted - 1) else {
        eprintln!(
            "{path}: no subfile {wanted}; the file holds {}",
            spc.subfiles.len()
        );
        return std::process::ExitCode::FAILURE;
    };
    // Named even for a single-subfile file, so that a multifile record never
    // looks like an ordinary one.
    println!(
        "\nSubfile {wanted} of {} (z = {})",
        spc.subfiles.len(),
        sub.subheader.subtime
    );
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
