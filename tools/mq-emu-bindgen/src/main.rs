//! `cargo run -p mq-emu-bindgen` regenerates crates/mq-emu-sys/src/layouts/generated.rs.
//! `cargo run -p mq-emu-bindgen -- --check` fails instead of writing if it is out of date.
//! `--pdb <path>` overrides `$MQ_ROOT/build/bin/<config>/eqlib.pdb`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut check = false;
    let mut pdb = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check" => check = true,
            "--pdb" => pdb = args.next().map(std::path::PathBuf::from),
            other => {
                eprintln!("unknown argument {other:?}; usage: mq-emu-bindgen [--check] [--pdb <eqlib.pdb>]");
                return ExitCode::FAILURE;
            }
        }
    }

    match run(check, pdb) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("mq-emu-bindgen: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(check: bool, pdb: Option<std::path::PathBuf>) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let pdb = match pdb {
        Some(path) => path,
        None => mq_emu_bindgen::default_pdb()?,
    };
    let roots_path = mq_emu_bindgen::roots_file();
    let roots = mq_emu_bindgen::parse_roots(&std::fs::read_to_string(&roots_path)?);
    let generated = mq_emu_bindgen::generate(&pdb, &roots)?;
    let output = mq_emu_bindgen::output_file();

    let current = std::fs::read_to_string(&output).unwrap_or_default();
    if check {
        if current == generated {
            println!("{} is up to date", output.display());
            return Ok(ExitCode::SUCCESS);
        }
        eprintln!("{} is out of date; run `cargo run -p mq-emu-bindgen`", output.display());
        return Ok(ExitCode::FAILURE);
    }

    if current != generated {
        std::fs::write(&output, &generated)?;
    }
    println!("wrote {} ({} roots, {} lines)", output.display(), roots.len(), generated.lines().count());
    Ok(ExitCode::SUCCESS)
}
