//! Read-only enrollment gate for the installed Windows broker.
//! Validation has no filesystem, registry-write, pipe, SQLite or process effects.
use std::{io, path::Path};

const PRODUCT_ID: &str = "{F981F16C-7472-4F3F-99C5-7C0E47956624}";

#[derive(Clone, Default)]
struct Enrollment {
    owner_sid: Option<String>,
    owner_product: Option<String>,
    owner_root: Option<String>,
    owner_directory: Option<String>,
    machine_product: Option<String>,
    active_directory: Option<String>,
    uninstalling: Option<u32>,
}

/// Compare only unambiguous absolute local Windows paths. Do not canonicalize by
/// opening directories: a rejected broker must not traverse or create user data.
fn normalized(path: &str) -> Option<String> {
    let path = path.replace('/', "\\");
    let path = path.trim_end_matches('\\');
    let bytes = path.as_bytes();
    if bytes.len() < 4
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1..3] != *b":\\"
        || path.chars().any(char::is_control)
    {
        return None;
    }
    for component in path[3..].split('\\') {
        if component.is_empty()
            || matches!(component, "." | "..")
            || component.ends_with(['.', ' '])
            || component.contains([':', '<', '>', '"', '|', '?', '*'])
        {
            return None;
        }
    }
    Some(path.to_lowercase())
}

fn allowed(state: &Enrollment, sid: &str, executable: &str, program_files: &str) -> bool {
    if sid.is_empty()
        || state.owner_sid.as_deref() != Some(sid)
        || state.uninstalling != Some(0)
        || !state
            .owner_product
            .as_deref()
            .is_some_and(|v| v.eq_ignore_ascii_case(PRODUCT_ID))
        || !state
            .machine_product
            .as_deref()
            .is_some_and(|v| v.eq_ignore_ascii_case(PRODUCT_ID))
    {
        return false;
    }
    let Some(root) = state.owner_root.as_deref().and_then(normalized) else {
        return false;
    };
    let Some(owner) = state.owner_directory.as_deref().and_then(normalized) else {
        return false;
    };
    let Some(active) = state.active_directory.as_deref().and_then(normalized) else {
        return false;
    };
    let Some(executable) = normalized(executable) else {
        return false;
    };
    let Some(program_files) = normalized(program_files) else {
        return false;
    };
    if root != format!("{program_files}\\englishassist") || owner != active {
        return false;
    }
    let prefix = format!("{root}\\versions\\");
    let Some(version) = active.strip_prefix(&prefix) else {
        return false;
    };
    !version.is_empty()
        && !version.contains('\\')
        && executable == format!("{active}\\retype-learning-host.exe")
}

#[allow(unsafe_code)]
fn read_string(
    hive: windows::Win32::System::Registry::HKEY,
    name: &str,
    machine: bool,
) -> Option<String> {
    use windows::{
        core::{w, PCWSTR},
        Win32::System::Registry::*,
    };
    let name: Vec<u16> = name.encode_utf16().chain([0]).collect();
    let mut buffer = [0u16; 4096];
    let mut bytes = (buffer.len() * 2) as u32;
    let flags = RRF_RT_REG_SZ
        | if machine {
            RRF_SUBKEY_WOW6464KEY
        } else {
            REG_ROUTINE_FLAGS(0)
        };
    // SAFETY: Fixed product key, terminated value name and bounded UTF-16 output.
    let result = unsafe {
        RegGetValueW(
            hive,
            w!("Software\\EnglishAssist"),
            PCWSTR(name.as_ptr()),
            flags,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    let length = bytes as usize / 2;
    if result.is_err()
        || bytes % 2 != 0
        || !(2..=buffer.len()).contains(&length)
        || buffer[length - 1] != 0
        || buffer[..length - 1].contains(&0)
    {
        return None;
    }
    String::from_utf16(&buffer[..length - 1]).ok()
}

#[allow(unsafe_code)]
fn read_uninstalling() -> Option<u32> {
    use windows::{core::w, Win32::System::Registry::*};
    let mut value = 0u32;
    let mut bytes = 4;
    // SAFETY: DWORD-only request, correctly sized output; failures deny startup.
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!("Software\\EnglishAssist"),
            w!("Uninstalling"),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut bytes),
        )
    };
    (result.is_ok() && bytes == 4).then_some(value)
}

pub(crate) fn require_current(sid: &str, executable: &Path) -> io::Result<()> {
    use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let state = Enrollment {
        owner_sid: read_string(HKEY_CURRENT_USER, "ProfileOwnerSid", false),
        owner_product: read_string(HKEY_CURRENT_USER, "ProfileProductId", false),
        owner_root: read_string(HKEY_CURRENT_USER, "ProfileInstallRoot", false),
        owner_directory: read_string(HKEY_CURRENT_USER, "ProfileInstallDir", false),
        machine_product: read_string(HKEY_LOCAL_MACHINE, "ProductId", true),
        active_directory: read_string(HKEY_LOCAL_MACHINE, "ActiveDir", true),
        uninstalling: read_uninstalling(),
    };
    let program_files = std::env::var("ProgramW6432").or_else(|_| std::env::var("ProgramFiles"));
    if executable
        .to_str()
        .zip(program_files.ok().as_deref())
        .is_some_and(|(executable, program_files)| allowed(&state, sid, executable, program_files))
    {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "English Assist enrollment is missing, mismatched, or uninstalling",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SID: &str = "S-1-5-21-10-20-30-1001";
    const ROOT: &str = "C:\\Program Files\\EnglishAssist";
    const ACTIVE: &str = "C:\\Program Files\\EnglishAssist\\versions\\0.2.0-fixture";
    const EXE: &str =
        "C:\\Program Files\\EnglishAssist\\versions\\0.2.0-fixture\\retype-learning-host.exe";
    const PROGRAM_FILES: &str = "C:\\Program Files";
    fn enrolled() -> Enrollment {
        Enrollment {
            owner_sid: Some(SID.into()),
            owner_product: Some(PRODUCT_ID.into()),
            owner_root: Some(ROOT.into()),
            owner_directory: Some(ACTIVE.into()),
            machine_product: Some(PRODUCT_ID.into()),
            active_directory: Some(ACTIVE.into()),
            uninstalling: Some(0),
        }
    }
    #[test]
    fn own_current_enrollment_allows_only_its_installed_broker() {
        assert!(allowed(&enrolled(), SID, EXE, PROGRAM_FILES));
        let mut state = enrolled();
        state.owner_directory = Some(ACTIVE.to_uppercase().replace('\\', "/") + "/");
        assert!(allowed(&state, SID, EXE, PROGRAM_FILES));
        assert!(!allowed(
            &state,
            SID,
            &EXE.replace("0.2.0-fixture", "old-version"),
            PROGRAM_FILES
        ));
        assert!(!allowed(
            &state,
            SID,
            &EXE.replace("retype-learning-host.exe", "other.exe"),
            PROGRAM_FILES
        ));
    }
    #[test]
    fn missing_owner_fields_and_uninstalling_never_allow_restart() {
        for field in 0..6 {
            let mut state = enrolled();
            let fields = [
                &mut state.owner_sid,
                &mut state.owner_product,
                &mut state.owner_root,
                &mut state.owner_directory,
                &mut state.machine_product,
                &mut state.active_directory,
            ];
            *fields
                .into_iter()
                .nth(field)
                .unwrap_or_else(|| unreachable!()) = None;
            assert!(!allowed(&state, SID, EXE, PROGRAM_FILES));
        }
        for flag in [None, Some(1), Some(2), Some(u32::MAX)] {
            let mut state = enrolled();
            state.uninstalling = flag;
            assert!(!allowed(&state, SID, EXE, PROGRAM_FILES));
        }
    }
    #[test]
    fn foreign_user_product_or_installation_is_rejected() {
        assert!(!allowed(
            &enrolled(),
            "S-1-5-21-10-20-30-9999",
            EXE,
            PROGRAM_FILES
        ));
        for foreign in 0..4 {
            let mut state = enrolled();
            match foreign {
                0 => state.owner_product = Some("other-product".into()),
                1 => state.machine_product = Some("other-product".into()),
                2 => state.owner_root = Some("C:\\Program Files\\Other".into()),
                _ => state.active_directory = Some(ACTIVE.replace("fixture", "new")),
            }
            assert!(!allowed(&state, SID, EXE, PROGRAM_FILES));
        }
    }
    #[test]
    fn ambiguous_paths_and_payload_descendants_cannot_pass_identity_checks() {
        for bad in [
            "relative\\EnglishAssist",
            "\\\\server\\share",
            "C:\\x\\..\\Program Files",
            "C:\\Program Files.",
            "C:\\Program Files ",
            "C:\\x:stream",
            "C:\\x\\\\y",
            "C:\\x\0y",
        ] {
            assert!(normalized(bad).is_none(), "{bad:?}");
        }
        let mut state = enrolled();
        let nested = format!("{ACTIVE}\\nested");
        state.active_directory = Some(nested.clone());
        state.owner_directory = Some(nested.clone());
        assert!(!allowed(
            &state,
            SID,
            &format!("{nested}\\retype-learning-host.exe"),
            PROGRAM_FILES
        ));
        let relocated = "C:\\Temp\\EnglishAssist\\versions\\0.2.0-fixture";
        state.owner_root = Some("C:\\Temp\\EnglishAssist".into());
        state.active_directory = Some(relocated.into());
        state.owner_directory = Some(relocated.into());
        assert!(!allowed(
            &state,
            SID,
            &format!("{relocated}\\retype-learning-host.exe"),
            PROGRAM_FILES
        ));
    }
}
