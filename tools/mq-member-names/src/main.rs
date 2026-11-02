//! `cargo run -p mq-member-names` regenerates crates/mq-emu/src/datatypes/macroquest_members.rs.
//! `cargo run -p mq-member-names -- --check` fails instead of writing if it is out of date.
//! `--mq-root <path>` overrides `$MQ_ROOT`.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut check = false;
    let mut root = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--check" => check = true,
            "--mq-root" => root = args.next().map(std::path::PathBuf::from),
            other => {
                eprintln!("unknown argument {other:?}; usage: mq-member-names [--check] [--mq-root <path>]");
                return ExitCode::FAILURE;
            }
        }
    }

    match run(check, root) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("mq-member-names: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(check: bool, root: Option<std::path::PathBuf>) -> Result<ExitCode, Box<dyn std::error::Error>> {
    let root = match root {
        Some(path) => path,
        None => mq_member_names::default_root()?,
    };
    let names = mq_member_names::scan(&root)?;
    let generated = mq_member_names::emit(&names);
    let output = mq_member_names::output_file();

    let current = std::fs::read_to_string(&output).unwrap_or_default();
    if check {
        if current == generated {
            println!("{} is up to date", output.display());
            return Ok(ExitCode::SUCCESS);
        }
        eprintln!("{} is out of date; run `cargo run -p mq-member-names`", output.display());
        return Ok(ExitCode::FAILURE);
    }

    if current != generated {
        std::fs::write(&output, &generated)?;
    }
    println!("wrote {} ({} members, {} methods)", output.display(), names.members.len(), names.methods.len());
    Ok(ExitCode::SUCCESS)
}
