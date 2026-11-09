//! Input DWORD preferences use one existing file outside AppData virtualization.
//! Strings retain the legacy registry representation; DWORDs never fall back to it.
#![allow(unsafe_code)]
use windows::{
    core::{w, Result, PCWSTR},
    Win32::{
        Foundation::*,
        System::{Com::CoTaskMemFree, Memory::*, Registry::*},
        UI::Shell::{FOLDERID_Profile, SHGetKnownFolderPath, KF_FLAG_DONT_VERIFY},
    },
};

const PRODUCT: PCWSTR = w!("Software\\EnglishAssist");
const OPTIONS: u32 = REG_OPTION_DONT_VIRTUALIZE.0;

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: this handle was opened by this object and is closed once.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}
fn open_at(path: PCWSTR) -> Result<Option<Key>> {
    let mut key = HKEY::default();
    // SAFETY: constant product path (or isolated fixture), owned output handle.
    let status = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            path,
            Some(OPTIONS),
            KEY_QUERY_VALUE,
            &mut key,
        )
    };
    if matches!(status, ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND) {
        return Ok(None);
    }
    status.ok()?;
    Ok(Some(Key(key)))
}
fn create_at(path: PCWSTR) -> Result<Key> {
    let mut key = HKEY::default();
    // SAFETY: use the SDK's explicit Open/Read/Write no-virtualization option;
    // if unavailable or denied, return its error without retrying a normal open.
    unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            path,
            None,
            None,
            REG_OPTION_DONT_VIRTUALIZE | REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            None,
            &mut key,
            None,
        )
        .ok()?;
    }
    Ok(Key(key))
}
impl Key {
    fn string(&self, name: PCWSTR) -> Result<Option<String>> {
        let mut value = [0u16; 32768];
        let mut bytes = (value.len() * 2) as u32;
        // SAFETY: bounded UTF-16 output, REG_SZ only, native handle and no subkey.
        let status = unsafe {
            RegGetValueW(
                self.0,
                None,
                name,
                RRF_RT_REG_SZ,
                None,
                Some(value.as_mut_ptr().cast()),
                Some(&mut bytes),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        status.ok()?;
        if bytes < 2 || bytes as usize > value.len() * 2 || bytes % 2 != 0 {
            return Err(ERROR_INVALID_DATA.into());
        }
        let units = bytes as usize / 2;
        if value[units - 1] != 0 {
            return Err(ERROR_INVALID_DATA.into());
        }
        let text = String::from_utf16(&value[..units - 1])
            .map_err(|_| windows::core::Error::from(ERROR_INVALID_DATA))?;
        if text.contains('\0') {
            return Err(ERROR_INVALID_DATA.into());
        }
        Ok(Some(text))
    }
    fn set_string(&self, name: PCWSTR, value: &str) -> Result<()> {
        if value.contains('\0') {
            return Err(ERROR_INVALID_DATA.into());
        }
        let units: Vec<u16> = value.encode_utf16().chain([0]).collect();
        if units.len() > 32768 {
            return Err(ERROR_INVALID_DATA.into());
        }
        // SAFETY: initialized UTF-16 units exposed as bytes for this synchronous call.
        let bytes =
            unsafe { std::slice::from_raw_parts(units.as_ptr().cast::<u8>(), units.len() * 2) };
        unsafe { RegSetValueExW(self.0, name, None, REG_SZ, Some(bytes)).ok() }
    }
}

pub fn read_dword(name: PCWSTR) -> Result<Option<u32>> {
    let index = slot(name)?;
    mapped()?.read(index)
}
pub fn write_dword(name: PCWSTR, value: u32) -> Result<()> {
    let index = slot(name)?;
    mapped()?.write(index, value)
}
pub fn read_string(name: PCWSTR) -> Result<Option<String>> {
    open_at(PRODUCT)?.map_or(Ok(None), |key| key.string(name))
}
pub fn write_string(name: PCWSTR, value: &str) -> Result<()> {
    create_at(PRODUCT)?.set_string(name, value)
}

use std::{
    fs::{File, OpenOptions},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicI32, AtomicU32, Ordering},
        Mutex, OnceLock,
    },
};

pub const MAGIC: &[u8; 8] = b"EAINPF01";
pub const CAPACITY: usize = 32;
pub const FILE_BYTES: usize = 32 + CAPACITY * 8;
pub const NAMES: &[&str] = &[
    "PinyinScheme",
    "EnglishDisabled",
    "EnglishSpellingDisabled",
    "EnglishPinyinDisabled",
    "EnglishSegmentationDisabled",
    "EnglishPhrasesDisabled",
    "EnglishAutoSpaceDisabled",
    "StatusBarDisabled",
    "StatusBarX",
    "StatusBarY",
    "StatusBarHasPosition",
    "CandidateTheme",
    "CandidateFontSize",
    "CandidateWidth",
    "CandidateCompact",
    "CandidateHideMeanings",
    "CandidateHidePartOfSpeech",
    "CandidateShowPhonetics",
    "CandidateHideKeyHints",
    "SymbolCompletionEnabled",
    "ManualWordsGeneration",
    "DictionaryGeneration",
    "EnabledDictionaryPacks",
];
static MAPPING: OnceLock<Mapping> = OnceLock::new();
static ATTEMPTED: OnceLock<()> = OnceLock::new();
static INITIALIZING: Mutex<()> = Mutex::new(());
static FAILURE: AtomicI32 = AtomicI32::new(E_FAIL.0);

fn invalid() -> windows::core::Error {
    ERROR_INVALID_DATA.into()
}
fn io_error(error: std::io::Error) -> windows::core::Error {
    windows::core::Error::from_hresult(windows::core::HRESULT::from_win32(
        error.raw_os_error().unwrap_or(13) as u32,
    ))
}
fn profile_path() -> Result<PathBuf> {
    // SAFETY: query the native user's profile, not a host-overridable environment
    // variable; release the COM-allocated path on every conversion outcome.
    unsafe {
        let value = SHGetKnownFolderPath(&FOLDERID_Profile, KF_FLAG_DONT_VERIFY, None)?;
        let converted = value.to_string();
        CoTaskMemFree(Some(value.0.cast()));
        let root = PathBuf::from(converted?);
        if !root.is_absolute() {
            return Err(invalid());
        }
        Ok(root.join(".EnglishAssist").join("input-preferences-v1.bin"))
    }
}
/// Installation creates this file. Retry only at activation/focus boundaries;
/// failed reads cache the error rather than reopening a file on every key.
pub fn initialize() -> Result<()> {
    if MAPPING.get().is_some() {
        return Ok(());
    }
    let _guard = INITIALIZING
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if MAPPING.get().is_some() {
        return Ok(());
    }
    let result: Result<()> = (|| {
        let mapping = Mapping::open(&profile_path()?)?;
        let _ = MAPPING.set(mapping);
        Ok(())
    })();
    if let Err(error) = &result {
        FAILURE.store(error.code().0, Ordering::Release);
    }
    result
}
fn mapped() -> Result<&'static Mapping> {
    if let Some(mapping) = MAPPING.get() {
        return Ok(mapping);
    }
    ATTEMPTED.get_or_init(|| {
        let _ = initialize();
    });
    MAPPING.get().ok_or_else(|| {
        windows::core::Error::from_hresult(windows::core::HRESULT(FAILURE.load(Ordering::Acquire)))
    })
}
fn slot(name: PCWSTR) -> Result<usize> {
    if name.is_null() {
        return Err(ERROR_INVALID_PARAMETER.into());
    }
    // SAFETY: callers provide a NUL-terminated preference name. Comparison stops
    // at the first mismatch (including its terminator), without allocating.
    unsafe {
        NAMES
            .iter()
            .position(|expected| {
                expected
                    .bytes()
                    .enumerate()
                    .all(|(i, c)| *name.0.add(i) == c as u16)
                    && *name.0.add(expected.len()) == 0
            })
            .ok_or_else(|| ERROR_INVALID_PARAMETER.into())
    }
}
struct Mapping {
    view: MEMORY_MAPPED_VIEW_ADDRESS,
    handle: HANDLE,
    _file: File,
}
// SAFETY: immutable header plus aligned AtomicU32 cells are the only mapped
// accesses. The view and its file stay alive until this object is dropped.
unsafe impl Send for Mapping {}
unsafe impl Sync for Mapping {}
impl Drop for Mapping {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(self.view);
            let _ = CloseHandle(self.handle);
        }
    }
}
impl Mapping {
    fn open(path: &Path) -> Result<Self> {
        // No create/truncate. Do not follow a replaced preference-file symlink.
        let parent = path.parent().ok_or_else(invalid)?;
        for ancestor in parent.ancestors() {
            if std::fs::symlink_metadata(ancestor)
                .map_err(io_error)?
                .file_attributes()
                & 0x400
                != 0
            {
                return Err(invalid());
            }
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(7)
            .custom_flags(0x0020_0000)
            .open(path)
            .map_err(io_error)?;
        let metadata = file.metadata().map_err(io_error)?;
        if !metadata.is_file()
            || metadata.len() != FILE_BYTES as u64
            || metadata.file_attributes() & 0x400 != 0
        {
            return Err(invalid());
        }
        // SAFETY: fixed existing file, unnamed mapping (no global named object),
        // with a bounded view. Every cell is naturally aligned at page+32+8*i.
        unsafe {
            let handle = CreateFileMappingW(
                HANDLE(file.as_raw_handle()),
                None,
                PAGE_READWRITE,
                0,
                0,
                None,
            )?;
            let view = MapViewOfFile(handle, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, FILE_BYTES);
            if view.Value.is_null() {
                let error = windows::core::Error::from_thread();
                let _ = CloseHandle(handle);
                return Err(error);
            }
            let mapping = Self {
                view,
                handle,
                _file: file,
            };
            let header = std::slice::from_raw_parts(mapping.view.Value.cast::<u8>(), 32);
            if &header[..8] != MAGIC
                || header[8..12] != 1u32.to_le_bytes()
                || header[12..16] != (CAPACITY as u32).to_le_bytes()
                || header[16..].iter().any(|byte| *byte != 0)
            {
                return Err(invalid());
            }
            for i in 0..CAPACITY {
                let have = mapping.cell(i, false).load(Ordering::Acquire);
                if have > 1
                    || (i >= NAMES.len()
                        && (have != 0 || mapping.cell(i, true).load(Ordering::Acquire) != 0))
                {
                    return Err(invalid());
                }
            }
            Ok(mapping)
        }
    }
    fn cell(&self, index: usize, value: bool) -> &AtomicU32 {
        // SAFETY: private callers use validated slot indices. No non-atomic view
        // of these bytes is kept while another process can update them.
        unsafe {
            AtomicU32::from_ptr(
                self.view
                    .Value
                    .cast::<u8>()
                    .add(32 + index * 8 + usize::from(value) * 4)
                    .cast(),
            )
        }
    }
    fn read(&self, index: usize) -> Result<Option<u32>> {
        match self.cell(index, false).load(Ordering::Acquire) {
            0 => Ok(None),
            1 => Ok(Some(self.cell(index, true).load(Ordering::Acquire))),
            _ => Err(invalid()),
        }
    }
    fn write(&self, index: usize, value: u32) -> Result<()> {
        if self.cell(index, false).load(Ordering::Acquire) > 1 {
            return Err(invalid());
        }
        self.cell(index, true).store(value, Ordering::Release);
        self.cell(index, false).store(1, Ordering::Release);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bytes() -> Vec<u8> {
        let mut bytes = vec![0; FILE_BYTES];
        bytes[..8].copy_from_slice(MAGIC);
        bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&(CAPACITY as u32).to_le_bytes());
        bytes
    }
    struct Fixture(PathBuf);
    impl Fixture {
        fn new(data: &[u8]) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let root = std::env::temp_dir().join(format!(
                "english-assist-map-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&root).unwrap();
            let path = root.join("input-preferences-v1.bin");
            std::fs::write(&path, data).unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
            let _ = std::fs::remove_dir(self.0.parent().unwrap());
        }
    }
    #[test]
    fn shared_slots_preserve_none_full_dword_and_independent_updates() -> Result<()> {
        let fixture = Fixture::new(&bytes());
        let first = Mapping::open(&fixture.0)?;
        let second = Mapping::open(&fixture.0)?;
        assert_eq!(first.read(6)?, None);
        first.write(6, 0)?;
        assert_eq!(second.read(6)?, Some(0));
        second.write(8, u32::MAX)?;
        assert_eq!(first.read(8)?, Some(u32::MAX));
        std::thread::scope(|scope| {
            scope.spawn(|| {
                for value in 0..2000 {
                    first.write(6, value).unwrap();
                }
            });
            scope.spawn(|| {
                for value in 0..2000 {
                    second.write(7, value).unwrap();
                }
            });
        });
        assert_eq!(second.read(6)?, Some(1999));
        assert_eq!(first.read(7)?, Some(1999));
        drop(first);
        drop(second);
        let reopened = Mapping::open(&fixture.0)?;
        assert_eq!(reopened.read(8)?, Some(u32::MAX));
        assert_eq!(slot(w!("EnglishAutoSpaceDisabled"))?, 6);
        assert!(slot(w!("UnknownSetting")).is_err());
        Ok(())
    }
    #[test]
    fn mapped_preferences_reject_bad_schema_and_never_create_missing_file() {
        for change in 0..5 {
            let mut data = bytes();
            match change {
                0 => data[0] ^= 1,
                1 => data[8] = 2,
                2 => data[16] = 1,
                3 => data[32] = 2,
                _ => data.pop().map(|_| ()).unwrap(),
            };
            let fixture = Fixture::new(&data);
            assert!(Mapping::open(&fixture.0).is_err());
        }
        let fixture = Fixture::new(&bytes());
        let missing = fixture.0.with_file_name("missing.bin");
        assert!(Mapping::open(&missing).is_err());
        assert!(!missing.exists());
    }
}
