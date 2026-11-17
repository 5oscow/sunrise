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

/// Reads a check plan from standard input and prints one reply line: 0 checked, 1 an error reply (a file it names didn't read), 2 no plan, 3 another contract.
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
    let (code, line) = checked(&plan);
    println!("{line}");
    ExitCode::from(code)
}

/// The exit code and the reply line for `plan`: a tool from before `native-check` prints no such line, which is how a caller tells.
fn checked(plan: &checks::Plan) -> (u8, serde_json::Value) {
    match checks::check(plan)
        .and_then(|reply| serde_json::to_value(reply).map_err(|e| e.to_string()))
    {
        Ok(reply) => (0, reply),
        Err(message) => (
            1,
            serde_json::json!({
                "contract": contract::CONTRACT,
                "error": {"kind": "unread", "message": message},
            }),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_that_cant_read_its_mods_replies_with_an_error_line() {
        // A file where the plan names a folder of mods: it doesn't read as one.
        let file = std::env::temp_dir().join(format!("native-check-file-{}", std::process::id()));
        std::fs::write(&file, "not a folder").unwrap();
        let (code, reply) = checked(&checks::Plan {
            contract: contract::CONTRACT,
            mods: file.clone(),
            build: "b".repeat(40),
            surface: None,
            build_data: None,
        });
        assert_eq!(code, 1);
        assert_eq!(reply["contract"], contract::CONTRACT);
        assert_eq!(reply["error"]["kind"], "unread");
        assert!(
            reply["error"]["message"]
                .as_str()
                .is_some_and(|m| !m.is_empty())
        );
        std::fs::remove_file(file).unwrap();
    }
}
