//! Explicit Windows-only checks using public fixture text, never document input.
//! Import accepts a redirected stdin line only; credentials are never CLI arguments.

#[cfg(windows)]
mod windows {
    use retype_ai::{
        config::{self, ApiKind, Config},
        protocol::AssistTask,
        secrets, service,
    };
    use std::{
        io::{self, BufRead, IsTerminal},
        time::Instant,
    };

    const OFFICIAL: &str = "https://api.deepseek.com/v1";
    const MAX_KEY_BYTES: usize = 512;
    type CheckResult<T> = Result<T, &'static str>;

    fn require_official(config: &Config) -> CheckResult<()> {
        let provider = config
            .selected()
            .map_err(|_| "No saved model is selected.")?;
        if provider.id != "deepseek"
            || provider.kind != ApiKind::Compatible
            || provider.base_url != OFFICIAL
        {
            return Err(
                "Only the selected deepseek provider at https://api.deepseek.com/v1 is allowed.",
            );
        }
        Ok(())
    }

    fn official_config() -> CheckResult<Config> {
        let config =
            config::load().map_err(|_| "Could not read the product's AI configuration.")?;
        require_official(&config)?;
        Ok(config)
    }

    fn read_key_line(reader: impl BufRead) -> CheckResult<Vec<u8>> {
        // The extra bytes accommodate CRLF and detect an overlong first line.
        let mut bytes = Vec::new();
        let read = reader
            .take((MAX_KEY_BYTES + 3) as u64)
            .read_until(b'\n', &mut bytes);
        if read.is_err() {
            bytes.fill(0);
            return Err("Could not read the credential from stdin.");
        }
        // Reaching the bounded read limit must not be hidden by trimming CRs.
        if bytes.len() > MAX_KEY_BYTES + 2 {
            bytes.fill(0);
            return Err("The stdin credential line exceeds the supported length.");
        }
        while matches!(bytes.last(), Some(b'\r' | b'\n')) {
            bytes.pop();
        }
        if bytes.is_empty()
            || bytes.len() > MAX_KEY_BYTES
            || !bytes.iter().all(|byte| byte.is_ascii_graphic())
        {
            bytes.fill(0);
            return Err("The stdin credential must be one nonempty ASCII line, without spaces, at most 512 bytes.");
        }
        Ok(bytes)
    }

    fn import_key() -> CheckResult<()> {
        official_config()?;
        let existing =
            secrets::key("deepseek").map_err(|_| "Could not read the saved credential.")?;
        if !existing.trim().is_empty() {
            println!("An existing DeepSeek credential was retained.");
            return Ok(());
        }
        let stdin = io::stdin();
        if stdin.is_terminal() {
            return Err("Import requires redirected stdin; do not put a credential in command arguments or type it into an echoing console.");
        }
        let mut bytes = read_key_line(stdin.lock())?;
        let result = (|| -> CheckResult<bool> {
            official_config()?;
            // Recheck after potentially waiting for stdin, so a credential
            // saved meanwhile is retained. Credential Manager has no CAS here;
            // this is not an OS-level atomic concurrent-writer guarantee.
            let current =
                secrets::key("deepseek").map_err(|_| "Could not read the saved credential.")?;
            if !current.trim().is_empty() {
                return Ok(false);
            }
            let value = std::str::from_utf8(&bytes)
                .map_err(|_| "The stdin credential has an invalid encoding.")?;
            secrets::save_key("deepseek", value)
                .map_err(|_| "Could not save the credential in Windows Credential Manager.")?;
            Ok(true)
        })();
        bytes.fill(0);
        std::hint::black_box(&bytes);
        if result? {
            println!("DeepSeek credential saved in Windows Credential Manager.");
        } else {
            println!("An existing DeepSeek credential was retained.");
        }
        Ok(())
    }

    fn check_samples() -> CheckResult<()> {
        let config = official_config()?;
        let provider = config
            .selected()
            .map_err(|_| "No saved model is selected.")?;
        let existing =
            secrets::key("deepseek").map_err(|_| "Could not read the saved credential.")?;
        let key = existing.trim();
        if key.is_empty() {
            return Err("No DeepSeek credential is saved; no request was sent.");
        }
        let mut failed = false;
        for (task, operation, input) in [
            (
                "GrammarCorrect",
                AssistTask::GrammarCorrect,
                "She go to school every day.",
            ),
            (
                "CompleteSentence",
                AssistTask::CompleteSentence,
                "I would like to",
            ),
        ] {
            // Use exactly the snapshot we validated and the same key we redact.
            // This is the production assistance HTTP implementation and retains
            // its fixed ten-second timeout, without reloading mutable settings.
            let start = Instant::now();
            let response = service::assist_with_key(provider, &config.model, input, operation, key);
            let elapsed_ms = start.elapsed().as_millis();
            match response {
                Ok(text) if !text.contains(key) => {
                    println!(
                        "{}",
                        serde_json::json!({"task":task,"elapsed_ms":elapsed_ms,"text":text})
                    );
                }
                _ => {
                    // Never print transport/provider errors, headers, debug
                    // responses or a reflected credential from the service.
                    println!(
                        "{}",
                        serde_json::json!({"task":task,"elapsed_ms":elapsed_ms,"status":"failed"})
                    );
                    failed = true;
                }
            }
        }
        if failed {
            Err("One or more public-sample checks failed.")
        } else {
            Ok(())
        }
    }

    pub fn run() -> CheckResult<()> {
        let mut args = std::env::args_os().skip(1);
        let first = args.next();
        if args.next().is_some() {
            return Err("Usage: deepseek-check [--import-key]. Credentials are accepted only through redirected stdin.");
        }
        match first {
            None => check_samples(),
            Some(flag) if flag == "--import-key" => import_key(),
            Some(_) => Err("Usage: deepseek-check [--import-key]. Credentials are accepted only through redirected stdin."),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn official_provider_guard_rejects_aliases_other_hosts_and_api_kinds() {
            let mut config = Config::default();
            assert!(require_official(&config).is_ok());
            config.providers[0].base_url = "https://api.deepseek.com.attacker.invalid/v1".into();
            assert!(require_official(&config).is_err());
            config.providers[0].base_url = OFFICIAL.into();
            config.providers[0].kind = ApiKind::Ollama;
            assert!(require_official(&config).is_err());
            config.providers[0].kind = ApiKind::Compatible;
            config.provider = "another-provider".into();
            assert!(require_official(&config).is_err());
        }
        #[test]
        fn stdin_line_is_bounded_and_does_not_accept_whitespace_or_empty_values() {
            assert_eq!(
                read_key_line(&b"synthetic-fixture\r\n"[..]).ok(),
                Some(b"synthetic-fixture".to_vec())
            );
            for value in [
                &b"\n"[..],
                &b" leading\n"[..],
                &b"two words\n"[..],
                &b"bad\0value\n"[..],
            ] {
                assert!(read_key_line(value).is_err());
            }
            assert!(read_key_line(&vec![b'x'; MAX_KEY_BYTES + 1][..]).is_err());
            let mut trailing_cr = b"synthetic-fixture".to_vec();
            trailing_cr.extend(vec![b'\r'; MAX_KEY_BYTES + 3]);
            assert!(read_key_line(&trailing_cr[..]).is_err());
        }
    }
}

#[cfg(windows)]
fn main() -> std::process::ExitCode {
    match windows::run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(not(windows))]
fn main() -> std::process::ExitCode {
    eprintln!("deepseek-check supports Windows only.");
    std::process::ExitCode::FAILURE
}
