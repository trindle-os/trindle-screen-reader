//! The trindle-screen-reader command: run, start, stop, status, dry-run, cues.

mod runner;

use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!("usage: trindle-screen-reader dry-run FILE.toml\n       trindle-screen-reader run [--text]\n       trindle-screen-reader start | stop | status\n       trindle-screen-reader cues DIR");
    ExitCode::from(2)
}

fn finish(r: Result<(), String>) -> ExitCode {
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::from(1)
        }
    }
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| format!("cannot start: {e}"))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(|s| s.as_str()) {
        Some("dry-run") => {
            let Some(path) = args.get(1) else { return usage() };
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("cannot read {path}: {e}");
                    return ExitCode::from(1);
                }
            };
            match reader::scenario::parse(&text) {
                Ok(s) => {
                    for l in reader::scenario::run(s) {
                        println!("{l}");
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("bad scenario: {e}");
                    ExitCode::from(1)
                }
            }
        }
        Some("run") => {
            let force_text = args.iter().any(|a| a == "--text");
            let rt = match runtime() {
                Ok(rt) => rt,
                Err(e) => return finish(Err(e)),
            };
            let local = tokio::task::LocalSet::new();
            match local.block_on(&rt, runner::run(force_text)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    reader::log::note(&format!("screen reader stopped: {e}"));
                    ExitCode::from(1)
                }
            }
        }
        Some("cues") => {
            let Some(dir) = args.get(1) else { return usage() };
            finish(reader::cues::write_all(std::path::Path::new(dir)).map_err(|e| e.to_string()))
        }
        Some("start") => finish(reader::supervisor::start()),
        Some("stop") => finish(reader::supervisor::stop()),
        Some("status") => {
            let on = reader::supervisor::is_running();
            println!("{}", if on { "running" } else { "not running" });
            if on { ExitCode::SUCCESS } else { ExitCode::from(1) }
        }
        Some("supervise") => finish(runtime().and_then(|rt| rt.block_on(reader::supervisor::supervise()))),
        _ => usage(),
    }
}
