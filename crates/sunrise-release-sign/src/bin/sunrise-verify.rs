//! sunrise-verify: only `verify`, for the install scripts (packaging/server/install.sh and install.ps1). It
//! is the same function `sunrise-release-sign verify` and the launcher run, with the keys built in from
//! release-keys.txt. The scripts pin this binary's SHA-256 (tools/ci/pin_verifier.py), so it takes no
//! `--key`, `--repository` or `--publishing`: nothing the caller passes can change who is trusted.
//!
//!     sunrise-verify verify --tag <tag> [--signature <file>] [--only <file>]... <folder>
//!
//! Built with the `test-key` feature (the install tests only, never a release), it also trusts the one key in
//! `SUNRISE_VERIFY_TEST_KEY`, instead of the built-in ones.

use std::process::ExitCode;
use sunrise_release_sign::folder::check_folder;

const REFUSED: [&str; 3] = ["--key", "--repository", "--publishing"];

fn main() -> ExitCode {
    #[allow(unused_mut)]
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(feature = "test-key")]
    if let Ok(key) = std::env::var("SUNRISE_VERIFY_TEST_KEY") {
        args.extend(["--key".into(), key]);
    }
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("sunrise-verify: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let Some((command, rest)) = args.split_first().filter(|(c, _)| *c == "verify") else {
        return Err("usage: sunrise-verify verify --tag <tag> [--signature <file>] [--only <file>]... <folder>".into());
    };
    if !cfg!(feature = "test-key")
        && let Some(option) = rest.iter().find(|a| REFUSED.contains(&a.as_str()))
    {
        return Err(format!("{command} does not take {option}"));
    }
    check_folder(rest)
}
