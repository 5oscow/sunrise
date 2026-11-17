//! `native-build`: native mods compiled against an unpacked SDK (docs/modding/src/reference/sdk/08-native-build.md).
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::OnceLock;
use sunrise_native_build::{check as checks, contract, host};

/// The plan's build folder, the host's root.
static WORK: OnceLock<PathBuf> = OnceLock::new();

/// Reads the plan from standard input and builds it: 0 built, 1 failed (a `failed` event says why), 2 no plan, 3 another contract.
pub fn run(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!(
            "usage: cargo sunrise-mod native-build < plan.json (docs/modding/src/reference/sdk/08-native-build.md)"
        );
        return ExitCode::from(2);
    }
    let mut input = std::io::stdin().lock();
    let plan = match contract::read_plan(&mut input) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(match error {
                contract::PlanError::Contract(_) => 3,
                contract::PlanError::Invalid(_) => 2,
            });
        }
    };
    let _ = WORK.set(plan.work.clone());
    host::install(host::Host {
        tr: |en| en,
        root: || WORK.get().cloned().unwrap_or_default(),
        get,
        mark_used: contract::note_used,
    });
    match contract::run(&plan, &mut input, &mut std::io::stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

/// Fetches through the proxy its environment names (`HTTPS_PROXY`, `HTTP_PROXY`, `ALL_PROXY`, `NO_PROXY`), as the launcher passes its own.
fn get(url: &str) -> Result<host::Body, String> {
    match sunrise_port_pack::net::agent(url).get(url).call() {
        Ok(response) => Ok(Some(Box::new(response.into_reader()))),
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(error) => Err(format!("{url}: {error}")),
    }
}

/// Reads a check plan from standard input and prints the reply: 0 checked, 1 a file it names didn't read, 2 no plan, 3 another contract.
pub fn check(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!(
            "usage: cargo sunrise-mod native-check < plan.json (docs/modding/src/reference/sdk/08-native-build.md)"
        );
        return ExitCode::from(2);
    }
    let plan: checks::Plan = match contract::read_line(&mut std::io::stdin().lock()) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(match error {
                contract::PlanError::Contract(_) => 3,
                contract::PlanError::Invalid(_) => 2,
            });
        }
    };
    match checks::check(&plan)
        .and_then(|reply| serde_json::to_string(&reply).map_err(|e| e.to_string()))
    {
        Ok(line) => {
            println!("{line}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
