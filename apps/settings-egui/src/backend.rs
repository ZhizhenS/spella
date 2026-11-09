//! Settings persistence and blocking operations, independent of the UI.
use crate::statistics;
#[path = "../../../platforms/windows/common/base_resources.rs"]
mod base_resources;
#[path = "../../../platforms/windows/common/manual_words.rs"]
pub mod manual_words;
pub use manual_words::ManualWord;
use std::os::windows::process::CommandExt;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::*,
        System::Registry::*,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const KEY: &str = "Software\\EnglishAssist";
pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}
pub fn prepare_base() -> std::result::Result<retype_resources::Status, String> {
    let root = base_resources::root().ok_or("找不到基础词库目录")?;
    let directory = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .map(|p| {
            if p.file_name().is_some_and(|name| name == "settings") {
                p.parent().map_or(p.clone(), Path::to_path_buf)
            } else {
                p
            }
        });
    retype_resources::migrate(&root, &base_resources::legacy(directory.as_deref()))
}
#[allow(unsafe_code)]
pub fn choose_resource_bundle() -> Result<Option<PathBuf>> {
    use windows::Win32::UI::Controls::Dialogs::*;
    let filter = wide("retype 词库包\0*.retype-resources\0所有文件\0*.*\0");
    let mut filename = [0u16; 32768];
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: windows::core::PWSTR(filename.as_mut_ptr()),
        nMaxFile: filename.len() as u32,
        Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };
    // SAFETY: buffers remain alive and writable for the synchronous native picker.
    if !unsafe { GetOpenFileNameW(&mut dialog) }.as_bool() {
        return Ok(None);
    }
    let length = filename
        .iter()
        .position(|c| *c == 0)
        .unwrap_or(filename.len());
    Ok(Some(PathBuf::from(String::from_utf16_lossy(
        &filename[..length],
    ))))
}

#[allow(unsafe_code)]
pub fn dword_at(key: &str, name: &str) -> Result<Option<u32>> {
    let key = wide(key);
    let name = wide(name);
    let mut value = 0u32;
    let mut size = 4;
    // SAFETY: Output is a DWORD, matching the advertised four-byte buffer.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&mut value as *mut u32).cast()),
            Some(&mut size),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    status.ok()?;
    Ok(Some(value))
}
#[allow(unsafe_code)]
pub fn set_at(key: &str, name: &str, value: u32) -> Result<()> {
    let key_name = wide(key);
    let name = wide(name);
    let mut key = HKEY::default();
    // SAFETY: Valid null-terminated names and owned key handle; closed on all paths.
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_name.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()?;
        let status = RegSetValueExW(
            key,
            PCWSTR(name.as_ptr()),
            None,
            REG_DWORD,
            Some(&value.to_le_bytes()),
        );
        let _ = RegCloseKey(key);
        status.ok()?;
    }
    Ok(())
}
pub fn set(name: &str, value: u32) -> Result<()> {
    set_at(KEY, name, value)
}
#[allow(unsafe_code)]
fn installed(name: &str) -> Result<Option<String>> {
    let name = wide(name);
    let mut data = vec![0u16; 32768];
    let mut size = (data.len() * 2) as u32;
    // SAFETY: Fixed UTF-16 buffer; always use the 64-bit installation registry view.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            w!("Software\\EnglishAssist"),
            PCWSTR(name.as_ptr()),
            RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
            None,
            Some(data.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status == ERROR_FILE_NOT_FOUND {
        return Ok(None);
    }
    status.ok()?;
    let end = data.iter().position(|v| *v == 0).unwrap_or(data.len());
    Ok(Some(String::from_utf16_lossy(&data[..end])))
}
pub fn local_root() -> Result<PathBuf> {
    Ok(
        PathBuf::from(std::env::var_os("LOCALAPPDATA").ok_or("无法找到本地数据目录")?)
            .join("EnglishAssist"),
    )
}
#[derive(Clone)]
pub struct Preferences {
    pub symbol_completion: bool,
    pub english: (bool, bool),
    pub english_pinyin: bool,
    pub english_segmentation: bool,
    pub english_phrases: bool,
    pub appearance: retype_types::CandidateAppearance,
    pub pack_mask: u32,
    pub version: String,
    pub directory: PathBuf,
}
pub fn preferences() -> Result<Preferences> {
    let directory = installed("ActiveDir")?
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf))
                .map(|dir| {
                    if dir.file_name().is_some_and(|name| name == "settings") {
                        dir.parent().unwrap_or(&dir).to_path_buf()
                    } else {
                        dir
                    }
                })
                .unwrap_or_default()
        });
    Ok(Preferences {
        symbol_completion: dword_at(KEY, "SymbolCompletionEnabled")?.unwrap_or(0) != 0,
        english: (
            dword_at(KEY, "EnglishDisabled")?.unwrap_or(0) == 0,
            dword_at(KEY, "EnglishSpellingDisabled")?.unwrap_or(0) == 0,
        ),
        english_pinyin: dword_at(KEY, "EnglishPinyinDisabled")?.unwrap_or(0) == 0,
        english_segmentation: dword_at(KEY, "EnglishSegmentationDisabled")?.unwrap_or(0) == 0,
        english_phrases: dword_at(KEY, "EnglishPhrasesDisabled")?.unwrap_or(0) == 0,
        appearance: appearance_at(KEY)?,
        pack_mask: dword_at(KEY, "EnabledDictionaryPacks")?.unwrap_or(0) & 0x7f,
        version: installed("Version")?.unwrap_or_else(|| env!("CARGO_PKG_VERSION").into()),
        directory,
    })
}

pub fn appearance_at(key: &str) -> Result<retype_types::CandidateAppearance> {
    use retype_types::{CandidateAppearance, CandidateTheme};
    Ok(CandidateAppearance {
        theme: match dword_at(key, "CandidateTheme")?.unwrap_or(0) {
            1 => CandidateTheme::Light,
            2 => CandidateTheme::Dark,
            _ => CandidateTheme::System,
        },
        font_size: dword_at(key, "CandidateFontSize")?.unwrap_or(17),
        width: dword_at(key, "CandidateWidth")?.unwrap_or(420),
        compact: dword_at(key, "CandidateCompact")?.unwrap_or(0) != 0,
        show_meanings: dword_at(key, "CandidateHideMeanings")?.unwrap_or(0) == 0,
        show_part_of_speech: dword_at(key, "CandidateHidePartOfSpeech")?.unwrap_or(0) == 0,
        show_phonetics: dword_at(key, "CandidateShowPhonetics")?.unwrap_or(0) != 0,
        show_key_hints: dword_at(key, "CandidateHideKeyHints")?.unwrap_or(0) == 0,
    }
    .normalized())
}

fn appearance_values(value: retype_types::CandidateAppearance) -> [(&'static str, u32); 8] {
    use retype_types::CandidateTheme;
    let value = value.normalized();
    [
        (
            "CandidateTheme",
            match value.theme {
                CandidateTheme::System => 0,
                CandidateTheme::Light => 1,
                CandidateTheme::Dark => 2,
            },
        ),
        ("CandidateFontSize", value.font_size),
        ("CandidateWidth", value.width),
        ("CandidateCompact", u32::from(value.compact)),
        ("CandidateHideMeanings", u32::from(!value.show_meanings)),
        (
            "CandidateHidePartOfSpeech",
            u32::from(!value.show_part_of_speech),
        ),
        ("CandidateShowPhonetics", u32::from(value.show_phonetics)),
        ("CandidateHideKeyHints", u32::from(!value.show_key_hints)),
    ]
}

pub fn save_appearance(value: retype_types::CandidateAppearance) -> Result<()> {
    let previous = appearance_at(KEY)?;
    save_appearance_with(previous, value, set)
}

fn save_appearance_with(
    previous: retype_types::CandidateAppearance,
    value: retype_types::CandidateAppearance,
    mut write: impl FnMut(&str, u32) -> Result<()>,
) -> Result<()> {
    for (name, value) in appearance_values(value) {
        if let Err(error) = write(name, value) {
            let mut rollback_failed = false;
            for (old_name, old_value) in appearance_values(previous) {
                rollback_failed |= write(old_name, old_value).is_err();
            }
            return if rollback_failed {
                Err(format!("{error}；部分外观设置恢复失败，请重新打开设置确认").into())
            } else {
                Err(error)
            };
        }
    }
    Ok(())
}

pub fn system_dark() -> bool {
    dword_at(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
        "AppsUseLightTheme",
    )
    .ok()
    .flatten()
        == Some(0)
}

#[allow(unsafe_code)]
fn set_string(name: &str, value: &str) -> Result<()> {
    let key_name = wide(KEY);
    let name = wide(name);
    let bytes: Vec<_> = value
        .encode_utf16()
        .chain([0])
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut key = HKEY::default();
    // SAFETY: Owned key, fixed product namespace, UTF-16 value including terminator.
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_name.as_ptr()),
            None,
            None,
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()?;
        let result = RegSetValueExW(key, PCWSTR(name.as_ptr()), None, REG_SZ, Some(&bytes));
        let _ = RegCloseKey(key);
        result.ok()?;
    }
    Ok(())
}

pub fn manual_words_path() -> Result<PathBuf> {
    Ok(local_root()?.join("words/manual.json"))
}

fn bounded_bytes(path: &Path) -> Result<Vec<u8>> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take((manual_words::MAX_FILE_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > manual_words::MAX_FILE_BYTES {
        return Err("词库文件超过 1 MB".into());
    }
    Ok(bytes)
}

pub fn load_manual_words() -> Result<Vec<ManualWord>> {
    let path = manual_words_path()?;
    match bounded_bytes(&path) {
        Ok(bytes) => manual_words::decode(&bytes).map_err(Into::into),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            Ok(Vec::new())
        }
        Err(error) => Err(error),
    }
}

/// Preserve the old file before replacing it atomically; a failed publication
/// restores the old file so the UI and other processes keep the same dictionary.
fn save_manual_with(
    path: &Path,
    words: &[ManualWord],
    publish: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let bytes = manual_words::encode(words)?;
    let parent = path.parent().ok_or("个人词库路径无效")?;
    fs::create_dir_all(parent)?;
    let temp = path.with_file_name(format!("manual-{}.tmp", std::process::id()));
    let backup = path.with_extension("json.bak");
    let existed = path.exists();
    if existed {
        fs::copy(path, &backup)?;
    }
    let write = (|| -> Result<()> {
        let mut file = fs::File::create(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)?;
        publish()
    })();
    if let Err(error) = write {
        let _ = fs::remove_file(&temp);
        let rollback = if existed {
            fs::copy(&backup, &temp).and_then(|_| fs::rename(&temp, path))
        } else if path.exists() {
            fs::remove_file(path)
        } else {
            Ok(())
        };
        return if rollback.is_err() {
            Err(format!("保存失败：{error}；自动恢复失败，请保留 manual.json.bak").into())
        } else {
            Err(error)
        };
    }
    Ok(())
}

pub fn save_manual_words(words: &[ManualWord]) -> Result<()> {
    let path = manual_words_path()?;
    save_manual_with(&path, words, || {
        set_string("ManualWordsPath", &path.to_string_lossy())?;
        let next = dword_at(KEY, "ManualWordsGeneration")?
            .unwrap_or(0)
            .wrapping_add(1);
        set("ManualWordsGeneration", next)?;
        Ok(())
    })
}

pub fn import_manual_words(path: &Path) -> Result<Vec<ManualWord>> {
    let bytes = bounded_bytes(path)?;
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        return manual_words::decode(&bytes).map_err(Into::into);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "请使用 UTF-8 编码的 TXT/TSV 文件")?;
    let mut words = Vec::new();
    for line in text
        .trim_start_matches('\u{feff}')
        .lines()
        .filter(|line| !line.trim().is_empty())
    {
        let (word, comment) = line.split_once('\t').unwrap_or((line, ""));
        words.push(ManualWord {
            word: word.trim().into(),
            comment: comment.trim().into(),
        });
    }
    manual_words::validate(&words)?;
    Ok(words)
}

pub fn export_manual_words(path: &Path, words: &[ManualWord]) -> Result<()> {
    let bytes = manual_words::encode(words)?;
    let mut file = fs::File::create(path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(())
}

#[allow(unsafe_code)]
pub fn choose_manual_words_file(save: bool) -> Result<Option<PathBuf>> {
    use windows::Win32::UI::Controls::Dialogs::*;
    let filter = wide(if save {
        "英文助手词库 JSON\0*.json\0\0"
    } else {
        "英文助手词库\0*.json;*.txt;*.tsv\0所有文件\0*.*\0\0"
    });
    let default = wide("english-assist-words.json");
    let mut filename = [0u16; 32768];
    if save {
        filename[..default.len()].copy_from_slice(&default);
    }
    let extension = wide("json");
    let mut dialog = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        lpstrFile: windows::core::PWSTR(filename.as_mut_ptr()),
        nMaxFile: filename.len() as u32,
        lpstrDefExt: PCWSTR(extension.as_ptr()),
        Flags: OFN_NOCHANGEDIR
            | OFN_PATHMUSTEXIST
            | if save {
                OFN_OVERWRITEPROMPT
            } else {
                OFN_FILEMUSTEXIST
            },
        ..Default::default()
    };
    // SAFETY: Synchronous native dialog owns no buffers; all references remain live.
    let chosen = unsafe {
        if save {
            GetSaveFileNameW(&mut dialog)
        } else {
            GetOpenFileNameW(&mut dialog)
        }
    };
    if !chosen.as_bool() {
        return Ok(None);
    }
    let length = filename
        .iter()
        .position(|&c| c == 0)
        .unwrap_or(filename.len());
    Ok(Some(PathBuf::from(String::from_utf16_lossy(
        &filename[..length],
    ))))
}

pub use retype_resources::packs::Pack;
pub fn packs() -> Result<Vec<Pack>> {
    Ok(retype_resources::packs::catalog()?)
}
pub fn checksum(path: &Path) -> Result<String> {
    Ok(retype_resources::checksum(path)?)
}
pub fn pack_installed(root: &Path, pack: &Pack) -> bool {
    let expected = fs::read_to_string(root.join(format!("{}.sha256", pack.id))).unwrap_or_default();
    let expected = expected.trim();
    expected.len() == 64
        && checksum(&root.join(format!("{}.bin", pack.id))).is_ok_and(|value| value == expected)
}
fn pack_preference(index: usize, enabled: bool) -> Result<()> {
    if index >= 7 {
        return Err("无效词库".into());
    }
    let mask = dword_at(KEY, "EnabledDictionaryPacks")?.unwrap_or(0);
    set(
        "EnabledDictionaryPacks",
        if enabled {
            mask | (1 << index)
        } else {
            mask & !(1 << index)
        },
    )?;
    set(
        "DictionaryGeneration",
        dword_at(KEY, "DictionaryGeneration")?
            .unwrap_or(0)
            .wrapping_add(1),
    )
}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err("下载已取消".into())
    } else {
        Ok(())
    }
}
pub fn set_pack(
    index: usize,
    enabled: bool,
    cancel: &AtomicBool,
    progress: impl FnMut(f32),
) -> Result<()> {
    let packs = packs()?;
    let pack = packs.get(index).ok_or("无效词库")?;
    let root = local_root()?.join("dict-packs");
    if enabled && !pack_installed(&root, pack) {
        download_pack(&root, pack, cancel, progress)?;
    }
    cancelled(cancel)?;
    pack_preference(index, enabled)
}
pub fn sync_enabled_packs() -> Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    let root = local_root()?.join("dict-packs");
    fs::create_dir_all(&root)?;
    let Ok(_lock) = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(0)
        .open(root.join("sync-download.lock"))
    else {
        return Ok(());
    };
    let cancel = AtomicBool::new(false);
    for (index, pack) in packs()?.iter().enumerate() {
        if preferences()?.pack_mask & (1 << index) != 0 && !pack_installed(&root, pack) {
            download_pack(&root, pack, &cancel, |_| {})?;
            set(
                "DictionaryGeneration",
                dword_at(KEY, "DictionaryGeneration")?
                    .unwrap_or(0)
                    .wrapping_add(1),
            )?;
        }
    }
    Ok(())
}
fn download_pack(
    root: &Path,
    pack: &Pack,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f32),
) -> Result<()> {
    fs::create_dir_all(root)?;
    let nonce = chrono::Local::now().timestamp_micros();
    let raw = root.join(format!("{}-{nonce}.yaml", pack.id));
    let binary = raw.with_extension("bin");
    let result = (|| -> Result<()> {
        cancelled(cancel)?;
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(300)))
            .timeout_connect(Some(Duration::from_secs(20)))
            .build()
            .into();
        let url = pack.source_url();
        let mut response = agent.get(&url).call()?;
        let mut reader = response.body_mut().as_reader();
        let mut output = fs::File::create(&raw)?;
        let mut buffer = [0u8; 32768];
        let mut received = 0u64;
        loop {
            cancelled(cancel)?;
            let count = reader.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            received += count as u64;
            if received > pack.bytes || received > 8 * 1024 * 1024 {
                return Err("词库超过大小限制".into());
            }
            output.write_all(&buffer[..count])?;
            progress(received as f32 / pack.bytes as f32);
        }
        output.sync_all()?;
        drop(output);
        cancelled(cancel)?;
        retype_resources::packs::compile_source(&pack.id, &raw, &binary)?;
        cancelled(cancel)?;
        let digest = checksum(&binary)?;
        replace(&binary, &root.join(format!("{}.bin", pack.id)))?;
        let checksum_path = root.join(format!("{}.sha256", pack.id));
        let staging = checksum_path.with_extension("sha256.tmp");
        fs::write(&staging, digest)?;
        replace(&staging, &checksum_path)?;
        Ok(())
    })();
    for path in [&raw, &binary] {
        let _ = fs::remove_file(path);
    }
    result
}
#[allow(unsafe_code)]
fn replace(source: &Path, target: &Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Storage::FileSystem::*;
    let source: Vec<_> = source.as_os_str().encode_wide().chain([0]).collect();
    let target: Vec<_> = target.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: Both are null-terminated paths under the owned data directory.
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )?;
    }
    Ok(())
}
#[derive(Clone)]
pub struct Offer {
    pub available: bool,
    pub installable: bool,
    pub version: Option<String>,
}
fn updater(arguments: &[&str]) -> Result<serde_json::Value> {
    let output = Command::new(preferences()?.directory.join("retype-updater.exe"))
        .args(arguments)
        .creation_flags(0x08000000)
        .output()?;
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    if !matches!(output.status.code(), Some(0 | 10)) {
        return Err(value["error"]
            .as_str()
            .unwrap_or("更新器失败")
            .to_string()
            .into());
    }
    Ok(value)
}
pub fn check() -> Result<Offer> {
    let prefs = preferences()?;
    let result = updater(&[
        "check",
        "--repo",
        "L-Chris/retype",
        "--current",
        &prefs.version,
        "--json",
    ])?;
    Ok(Offer {
        available: result["update_available"].as_bool().unwrap_or(false),
        installable: result["installable"].as_bool().unwrap_or(false),
        version: result["latest"]["version"].as_str().map(str::to_owned),
    })
}
pub fn skip(version: &str) -> Result<()> {
    skip_at(&local_root()?.join("updates"), version)
}
fn skip_at(root: &Path, version: &str) -> Result<()> {
    fs::create_dir_all(root)?;
    let path = root.join("state.json");
    let mut state = fs::read(&path)
        .ok()
        .and_then(|v| serde_json::from_slice::<serde_json::Value>(&v).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}));
    state["SkippedVersion"] = version.into();
    let staging = root.join(format!("state-{}.tmp", std::process::id()));
    fs::write(&staging, serde_json::to_vec(&state)?)?;
    replace(&staging, &path)
}
#[allow(unsafe_code)]
pub fn open(destination: &str) -> Result<()> {
    let destination = wide(destination);
    // SAFETY: Fixed action and a null-terminated destination, no shell command string.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(destination.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if result.0 as isize <= 32 {
        return Err("无法打开目标".into());
    }
    Ok(())
}
pub fn install(version: &str, mut stage: impl FnMut(&str)) -> Result<String> {
    let root = local_root()?
        .join("updates")
        .join(chrono::Local::now().timestamp_micros().to_string());
    fs::create_dir_all(&root)?;
    let directory = root.to_str().ok_or("更新目录编码无效")?;
    stage("正在下载并校验安装包…");
    let value = updater(&[
        "download",
        "--repo",
        "L-Chris/retype",
        "--json",
        "--timeout",
        "300",
        "--expected-version",
        version,
        "--out",
        directory,
    ])?;
    let installer = PathBuf::from(value["downloaded"].as_str().ok_or("更新器没有返回安装包")?);
    if !installer.is_file()
        || installer.extension().is_none_or(|v| v != "exe")
        || !installer.starts_with(&root)
    {
        return Err("无效安装包路径".into());
    }
    stage("正在安装；如弹出管理员权限提示，请确认。");
    let code = elevate(&installer)?;
    if code == 3010 {
        return Ok("安装已准备完成，请手动重启电脑后生效。".into());
    }
    if code != 0 {
        return Err(format!("安装失败，退出码：{code}").into());
    }
    stage("正在验证安装…");
    let prefs = preferences()?;
    let result = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(prefs.directory.join("update-verify.ps1"))
        .args(["-ExpectedVersion", version])
        .creation_flags(0x08000000)
        .output()?;
    if !result.status.success() {
        return Err(format!(
            "安装后验证失败：{}",
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    Ok(format!(
        "已安装 {version}。重新打开正在运行的应用即可使用新版。"
    ))
}
#[allow(unsafe_code)]
fn elevate(installer: &Path) -> Result<u32> {
    use windows::Win32::System::Threading::*;
    let file = wide(&installer.to_string_lossy());
    // SAFETY: All strings outlive ShellExecuteExW; process handle closed on every path.
    unsafe {
        let mut info=SHELLEXECUTEINFOW { cbSize:std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask:SEE_MASK_NOCLOSEPROCESS,lpVerb:w!("runas"),lpFile:PCWSTR(file.as_ptr()),
            lpParameters:w!("/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /NOCLOSEAPPLICATIONS /NORESTARTAPPLICATIONS /RESTARTEXITCODE=3010"),nShow:SW_HIDE.0,..Default::default() };
        ShellExecuteExW(&mut info)?;
        if info.hProcess.is_invalid() {
            return Err("安装器没有返回进程".into());
        }
        let wait = WaitForSingleObject(info.hProcess, 600_000);
        let mut code = 0;
        let result: Result<()> = if wait == WAIT_OBJECT_0 {
            GetExitCodeProcess(info.hProcess, &mut code).map_err(Into::into)
        } else {
            Err("安装等待超时，请检查安装日志".into())
        };
        let _ = CloseHandle(info.hProcess);
        result?;
        Ok(code)
    }
}

pub enum Task {
    BaseStatus,
    BaseInstall(Option<PathBuf>, Arc<AtomicBool>),
    Packs,
    SetPack(usize, bool, Arc<AtomicBool>),
    Statistics,
    Check,
    #[allow(dead_code)] // Legacy updater worker retained; the English-only UI never requests it.
    Install(String),
    #[allow(dead_code)]
    Skip(String),
}
pub enum Event {
    BaseStatus(std::result::Result<retype_resources::Status, String>),
    BaseProgress(f32),
    BaseDone(std::result::Result<retype_resources::Status, String>),
    Packs(Vec<bool>),
    PackProgress(f32),
    PackDone(std::result::Result<(), String>),
    Statistics(Box<statistics::Snapshot>),
    Checked(std::result::Result<Offer, String>),
    UpdateStage(String),
    Installed(std::result::Result<String, String>),
    Skipped(std::result::Result<(), String>),
    Error(String),
}
pub fn worker(
    ctx: eframe::egui::Context,
) -> (
    std::sync::mpsc::Sender<Task>,
    std::sync::mpsc::Receiver<Event>,
    Arc<AtomicBool>,
) {
    let (tasks, receiver) = std::sync::mpsc::channel();
    let (sender, events) = std::sync::mpsc::channel();
    let busy = Arc::new(AtomicBool::new(false));
    let worker_busy = Arc::clone(&busy);
    std::thread::spawn(move || {
        let root = local_root().unwrap_or_else(|_| std::env::temp_dir().join("EnglishAssist"));
        let mut statistics = statistics::Store::new(root.join("statistics"));
        let emit = |event| {
            let _ = sender.send(event);
            ctx.request_repaint();
        };
        for task in receiver {
            struct BusyGuard(Arc<AtomicBool>);
            impl Drop for BusyGuard {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::Relaxed);
                }
            }
            worker_busy.store(true, Ordering::Relaxed);
            let _guard = BusyGuard(Arc::clone(&worker_busy));
            match task {
                Task::BaseStatus => emit(Event::BaseStatus(prepare_base())),
                Task::BaseInstall(source, cancel) => emit(Event::BaseDone((|| {
                    let root = base_resources::root().ok_or("找不到基础词库目录")?;
                    if let Some(source) = source {
                        retype_resources::import_bundle(&root, &source, &cancel)
                    } else {
                        retype_resources::download(&root, &cancel, |p| emit(Event::BaseProgress(p)))
                    }
                })())),
                Task::Packs => match packs() {
                    Ok(packs) => emit(Event::Packs(
                        packs
                            .iter()
                            .map(|pack| pack_installed(&root.join("dict-packs"), pack))
                            .collect(),
                    )),
                    Err(e) => emit(Event::Error(e.to_string())),
                },
                Task::SetPack(index, enabled, cancel) => emit(Event::PackDone(
                    set_pack(index, enabled, &cancel, |p| emit(Event::PackProgress(p)))
                        .map_err(|e| e.to_string()),
                )),
                Task::Statistics => {
                    let now = chrono::Local::now().timestamp_millis();
                    match statistics.load(now) {
                        Ok(data) => emit(Event::Statistics(Box::new(data))),
                        Err(e) => emit(Event::Error(e.to_string())),
                    }
                }
                Task::Check => emit(Event::Checked(check().map_err(|e| e.to_string()))),
                Task::Install(version) => emit(Event::Installed(
                    install(&version, |text| emit(Event::UpdateStage(text.into())))
                        .map_err(|e| e.to_string()),
                )),
                Task::Skip(version) => {
                    emit(Event::Skipped(skip(&version).map_err(|e| e.to_string())))
                }
            }
        }
    });
    (tasks, events, busy)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TemporaryFiles(PathBuf);
    impl TemporaryFiles {
        fn new() -> Result<Self> {
            let path = std::env::temp_dir().join(format!(
                "english-assist-manual-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            ));
            fs::create_dir(&path)?;
            Ok(Self(path))
        }
    }
    impl Drop for TemporaryFiles {
        fn drop(&mut self) {
            if let Ok(entries) = fs::read_dir(&self.0) {
                for entry in entries.flatten() {
                    let _ = fs::remove_file(entry.path());
                }
            }
            let _ = fs::remove_dir(&self.0);
        }
    }

    #[test]
    fn manual_dictionary_backup_clear_and_failed_publication_restore_disk() -> Result<()> {
        let root = TemporaryFiles::new()?;
        let path = root.0.join("manual.json");
        let original = vec![ManualWord {
            word: "Codex".into(),
            comment: "项目名称".into(),
        }];
        save_manual_with(&path, &original, || Ok(()))?;
        let first_bytes = fs::read(&path)?;
        let updated = vec![ManualWord {
            word: "Hallelujah".into(),
            comment: "n. 赞美之声".into(),
        }];
        save_manual_with(&path, &updated, || Ok(()))?;
        assert_eq!(fs::read(path.with_extension("json.bak"))?, first_bytes);
        let updated_bytes = fs::read(&path)?;
        assert!(save_manual_with(&path, &original, || Err("publication denied".into())).is_err());
        assert_eq!(fs::read(&path)?, updated_bytes);
        save_manual_with(&path, &[], || Ok(()))?;
        assert!(manual_words::decode(&fs::read(&path)?)?.is_empty());
        assert_eq!(fs::read(path.with_extension("json.bak"))?, updated_bytes);
        fs::remove_file(&path)?;
        assert!(save_manual_with(&path, &original, || Err("publication denied".into())).is_err());
        assert!(
            !path.exists(),
            "failed first save must not leave a published file"
        );
        Ok(())
    }

    #[test]
    fn manual_dictionary_import_export_validation_preserves_existing_data() -> Result<()> {
        let root = TemporaryFiles::new()?;
        let text = root.0.join("import.tsv");
        fs::write(&text, "\u{feff}Codex\t项目名称\ncan't\t不能\n\n")?;
        let imported = import_manual_words(&text)?;
        assert_eq!(imported.len(), 2);
        assert_eq!(imported[0].comment, "项目名称");
        let exported = root.0.join("export.json");
        export_manual_words(&exported, &imported)?;
        assert_eq!(import_manual_words(&exported)?, imported);
        let before = fs::read(&exported)?;
        assert!(save_manual_with(
            &exported,
            &[ManualWord {
                word: "bad word".into(),
                comment: String::new()
            }],
            || panic!("invalid input must fail before publication")
        )
        .is_err());
        assert_eq!(fs::read(&exported)?, before);
        fs::write(&text, "Codex\nCODEX\n")?;
        assert!(import_manual_words(&text).is_err());
        fs::write(&text, vec![b'x'; manual_words::MAX_FILE_BYTES + 1])?;
        assert!(import_manual_words(&text).is_err());
        Ok(())
    }

    #[test]
    fn appearance_schema_normalizes_and_restores_all_values_after_write_failure() -> Result<()> {
        use retype_types::{CandidateAppearance, CandidateTheme};
        let previous = CandidateAppearance::default();
        let requested = CandidateAppearance {
            theme: CandidateTheme::Dark,
            font_size: 99,
            width: 1,
            compact: true,
            show_meanings: false,
            show_part_of_speech: false,
            show_phonetics: true,
            show_key_hints: false,
        };
        let mut stored: std::collections::HashMap<String, u32> = appearance_values(previous)
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect();
        let mut calls = 0;
        assert!(save_appearance_with(previous, requested, |key, value| {
            calls += 1;
            if calls == 4 {
                return Err("injected write failure".into());
            }
            stored.insert(key.into(), value);
            Ok(())
        })
        .is_err());
        for (key, value) in appearance_values(previous) {
            assert_eq!(stored[key], value);
        }
        save_appearance_with(previous, requested, |key, value| {
            stored.insert(key.into(), value);
            Ok(())
        })?;
        assert_eq!(stored["CandidateTheme"], 2);
        assert_eq!(stored["CandidateFontSize"], 22);
        assert_eq!(stored["CandidateWidth"], 320);
        assert_eq!(stored["CandidateHideMeanings"], 1);
        assert_eq!(stored["CandidateHidePartOfSpeech"], 1);
        assert_eq!(stored["CandidateShowPhonetics"], 1);
        assert_eq!(stored["CandidateHideKeyHints"], 1);
        Ok(())
    }
    #[test]
    fn manifest_matches_existing_tsf_pack_bits() -> Result<()> {
        let packs = packs()?;
        assert_eq!(
            packs
                .iter()
                .map(|pack| pack.id.as_str())
                .collect::<Vec<_>>(),
            ["mingren", "renming", "yiren", "diming", "yixue", "yaopin", "huaxue"]
        );
        assert!(packs
            .iter()
            .all(|pack| pack.bytes > 0 && pack.bytes < 8 * 1024 * 1024 && pack.sha256.len() == 64));
        Ok(())
    }
    #[test]
    fn checksum_detects_corruption_and_skip_keeps_existing_state() -> Result<()> {
        let root =
            std::env::temp_dir().join(format!("retype-egui-backend-test-{}", std::process::id()));
        fs::create_dir_all(&root)?;
        let pack = packs()?.remove(0);
        let binary = root.join(format!("{}.bin", pack.id));
        fs::write(&binary, b"test dictionary")?;
        fs::write(root.join(format!("{}.sha256", pack.id)), checksum(&binary)?)?;
        assert!(pack_installed(&root, &pack));
        fs::write(&binary, b"corrupt")?;
        assert!(!pack_installed(&root, &pack));
        fs::write(
            root.join("state.json"),
            br#"{"AutoCheck":false,"LastCheck":"retained"}"#,
        )?;
        skip_at(&root, "0.5.0")?;
        let state: serde_json::Value = serde_json::from_slice(&fs::read(root.join("state.json"))?)?;
        assert_eq!(state["AutoCheck"], false);
        assert_eq!(state["LastCheck"], "retained");
        assert_eq!(state["SkippedVersion"], "0.5.0");
        for entry in fs::read_dir(&root)? {
            fs::remove_file(entry?.path())?;
        }
        fs::remove_dir(root)?;
        Ok(())
    }
    #[test]
    #[ignore = "downloads the pinned upstream pack"]
    fn download_and_convert_without_changing_user_preferences() -> Result<()> {
        let root =
            std::env::temp_dir().join(format!("retype-pack-egui-smoke-{}", std::process::id()));
        let pack = packs()?.remove(0);
        let result = download_pack(&root, &pack, &AtomicBool::new(false), |_| {});
        if result.is_ok() {
            assert!(pack_installed(&root, &pack));
        }
        if root.exists() {
            for entry in fs::read_dir(&root)? {
                fs::remove_file(entry?.path())?;
            }
            fs::remove_dir(root)?;
        }
        result
    }
}
