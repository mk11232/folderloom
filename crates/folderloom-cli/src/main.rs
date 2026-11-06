use folderloom_core::{EmptyFolders, Job, Options, Progress, State};
use std::{
    io::{self, Write},
    path::PathBuf,
    process::ExitCode,
};
const USAGE: &str = "Usage:
  folderloom scan DATABASE ROOT... [--empty-folders include|ignore] [--batch-size N] [--interactive]
  folderloom resume DATABASE [--batch-size N] [--interactive]
  folderloom status DATABASE
  folderloom inventory DATABASE

DATABASE's parent must already exist outside all source roots.
Interactive mode commits a batch, then accepts next or pause on stdin (EOF pauses).
Status is the last saved observation, not a fresh verification. Running can mean interrupted.
Exit codes: 0 complete/paused/read successful; 2 finished with errors; 1 command/storage error.";
fn print_progress(p: &Progress) -> io::Result<()> {
    println!(
        "checkpoint generation={} state={:?} entries={} stale={} pending={} failed_directories={} failures={}",
        p.generation,
        p.state,
        p.entries,
        p.stale_entries,
        p.pending_directories,
        p.failed_directories,
        p.failures
    );
    io::stdout().flush()
}
fn run() -> Result<u8, Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let command = args.next().unwrap_or_default();
    if command == "--help" || command == "-h" || command.is_empty() {
        println!("{USAGE}");
        return Ok(0);
    }
    let database = PathBuf::from(args.next().ok_or("missing DATABASE; use --help")?);
    let mut roots = Vec::new();
    let mut options = Options::default();
    let mut budget = 256usize;
    let mut interactive = false;
    let mut positional = false;
    while let Some(arg) = args.next() {
        if !positional && arg == "--" {
            positional = true;
            continue;
        }
        if !positional && arg == "--batch-size" {
            budget = args
                .next()
                .ok_or("missing batch size")?
                .to_str()
                .ok_or("invalid batch size")?
                .parse()?;
        } else if !positional && arg == "--interactive" {
            interactive = true;
        } else if !positional && arg == "--empty-folders" {
            options.empty_folders = match args.next().as_deref() {
                Some(v) if v == "include" => EmptyFolders::Include,
                Some(v) if v == "ignore" => EmptyFolders::Ignore,
                _ => return Err("empty-folder policy must be include or ignore".into()),
            };
        } else if !positional && arg.as_encoded_bytes().starts_with(b"--") {
            return Err(format!("unknown option {arg:?}").into());
        } else {
            roots.push(PathBuf::from(arg));
        }
    }
    if command != "scan" && !roots.is_empty() {
        return Err("only scan accepts source roots".into());
    }
    if budget == 0 {
        return Err("batch size must be positive".into());
    }
    if command == "status" || command == "inventory" {
        let job = Job::open(&database)?;
        print_progress(&job.progress()?)?;
        println!("Saved observations only; resume starts a fresh validation generation.");
        for root in job.roots()? {
            println!("root={} path={:?}", root.id, root.path);
        }
        if command == "inventory" {
            for entry in job.inventory()? {
                println!(
                    "root={} path={:?} type={:?} bytes={} modified_ns={:?} target={:?} excluded={} stale={} directory_complete={}",
                    entry.root_id,
                    entry.relative_path,
                    entry.entry_type,
                    entry.size,
                    entry.modified_ns,
                    entry.link_target,
                    entry.excluded,
                    entry.stale,
                    entry.directory_complete
                );
            }
        }
        for failure in job.failures()? {
            println!(
                "failure generation={} root={} path={:?}: {}",
                failure.generation, failure.root_id, failure.relative_path, failure.message
            );
        }
        return Ok(0);
    }
    let mut job = if command == "scan" {
        Job::create(&database, &roots, options)?
    } else if command == "resume" {
        Job::resume(&database)?
    } else {
        return Err("unknown command; use --help".into());
    };
    loop {
        let progress = job.step(budget)?;
        print_progress(&progress)?;
        match progress.state {
            State::Complete | State::Paused => return Ok(0),
            State::FinishedWithErrors => return Ok(2),
            State::Running => {}
        }
        if interactive {
            let mut line = String::new();
            io::stdin().read_line(&mut line)?;
            match line.trim() {
                "next" => {}
                "pause" | "" => {
                    print_progress(&job.pause()?)?;
                    return Ok(0);
                }
                _ => {
                    job.pause()?;
                    return Err("expected next or pause; job paused".into());
                }
            }
        }
    }
}
fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("folderloom: {e}");
            ExitCode::FAILURE
        }
    }
}
