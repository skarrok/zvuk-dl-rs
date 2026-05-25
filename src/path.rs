use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

use const_fnv1a_hash::fnv1a_hash_32;

const MAX_COMPONENT_LEN: usize = 255;

#[cfg(target_os = "windows")]
pub const MAX_TOTAL_PATH_LEN: usize = 240;

#[cfg(target_os = "linux")]
const MAX_TOTAL_PATH_LEN: usize = 4096;

#[cfg(target_os = "macos")]
const MAX_TOTAL_PATH_LEN: usize = 1024;

#[cfg(not(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "macos"
)))]
const MAX_TOTAL_PATH_LEN: usize = 1024;

// Windows supports paths up to ~32767 chars when long paths are enabled
// (registry key LongPathsEnabled). Rust's std transparently prefixes paths
// over MAX_PATH with `\\?\`, so we can target this larger limit in that case.
#[cfg(target_os = "windows")]
const LONG_MAX_TOTAL_PATH_LEN: usize = 32_767;

/// Returns whether long paths are enabled for the system via the
/// `HKLM\SYSTEM\CurrentControlSet\Control\FileSystem\LongPathsEnabled` registry
/// value. Returns `false` if the value is missing or unreadable.
#[cfg(target_os = "windows")]
fn long_paths_enabled() -> bool {
    use winreg::RegKey;
    use winreg::enums::HKEY_LOCAL_MACHINE;

    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(r"SYSTEM\CurrentControlSet\Control\FileSystem")
        .and_then(|key| key.get_value::<u32, _>("LongPathsEnabled"))
        .is_ok_and(|value| value == 1)
}

/// The maximum total path length to target.
///
/// On Windows this is the larger long-path limit when `LongPathsEnabled` is
/// set in the registry, otherwise the conservative `MAX_PATH`-based default.
/// The result is detected once and cached. On other platforms it is the
/// platform default.
#[cfg(target_os = "windows")]
pub fn effective_max_total_path_len() -> usize {
    use std::sync::OnceLock;

    static CACHED: OnceLock<usize> = OnceLock::new();
    *CACHED.get_or_init(|| {
        if long_paths_enabled() {
            LONG_MAX_TOTAL_PATH_LEN
        } else {
            MAX_TOTAL_PATH_LEN
        }
    })
}

#[cfg(not(target_os = "windows"))]
pub const fn effective_max_total_path_len() -> usize {
    MAX_TOTAL_PATH_LEN
}

fn component_len(value: &str) -> usize {
    if cfg!(target_os = "windows") {
        value.encode_utf16().count()
    } else {
        value.len()
    }
}

fn path_len(path: &Path) -> usize {
    os_str_len(path.as_os_str())
}

fn os_str_len(s: &OsStr) -> usize {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;

        s.encode_wide().count()
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::ffi::OsStrExt;

        s.as_bytes().len()
    }
}

fn short_hash(value: &str) -> String {
    format!("{:016x}", fnv1a_hash_32(value.as_bytes(), None))
        .chars()
        .skip(8)
        .collect()
}

fn truncate_component(value: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }

    let mut result = String::new();
    let mut current_len = 0;

    for ch in value.chars() {
        let ch_len = if cfg!(target_os = "windows") {
            ch.len_utf16()
        } else {
            ch.len_utf8()
        };

        if current_len + ch_len > max_len {
            break;
        }

        result.push(ch);
        current_len += ch_len;
    }

    if result.is_empty() {
        String::from("_")
    } else {
        result
    }
}

#[cfg(target_os = "windows")]
fn is_reserved_name(value: &str) -> bool {
    let base = value.split('.').next().unwrap_or(value);
    let upper = base.to_ascii_uppercase();

    matches!(
        upper.as_str(),
        "CON"
            | "PRN"
            | "AUX"
            | "NUL"
            | "COM1"
            | "COM2"
            | "COM3"
            | "COM4"
            | "COM5"
            | "COM6"
            | "COM7"
            | "COM8"
            | "COM9"
            | "LPT1"
            | "LPT2"
            | "LPT3"
            | "LPT4"
            | "LPT5"
            | "LPT6"
            | "LPT7"
            | "LPT8"
            | "LPT9"
    )
}

#[cfg(not(target_os = "windows"))]
const fn is_reserved_name(_value: &str) -> bool {
    false
}

fn normalize_component(value: &Path, max_len: usize, seed: &str) -> String {
    let value = value.to_string_lossy();
    let max_len = max_len.max(1);

    let mut normalized = sanitize_path(&value).trim().to_string();

    if cfg!(target_os = "windows") {
        normalized = normalized.trim_end_matches([' ', '.']).to_string();
    }

    if normalized.is_empty() {
        normalized = String::from("_");
    }

    if is_reserved_name(&normalized) {
        normalized = format!("_{normalized}");
    }

    if component_len(&normalized) <= max_len {
        return normalized;
    }

    let suffix = format!("~{}", short_hash(seed));
    let suffix_len = component_len(&suffix);

    if suffix_len >= max_len {
        return truncate_component(&suffix, max_len);
    }

    let mut prefix = truncate_component(&normalized, max_len - suffix_len);

    if cfg!(target_os = "windows") {
        prefix = prefix.trim_end_matches([' ', '.']).to_string();
    }

    if prefix.is_empty() {
        prefix = String::from("_");
    }

    let mut shortened = format!("{prefix}{suffix}");
    if is_reserved_name(&shortened) {
        shortened = format!("_{shortened}");
    }

    if component_len(&shortened) > max_len {
        shortened = truncate_component(&shortened, max_len);
    }

    shortened
}

fn normalize_filename(
    stem: &OsStr,
    extension: &OsStr,
    max_len: usize,
    seed: &str,
) -> String {
    let max_len = max_len.max(1);
    let extension = extension.to_string_lossy();
    let extension = extension.trim_matches('.');
    let extension = format!(".{extension}");
    let extension_len = component_len(&extension);

    if extension_len >= max_len {
        return truncate_component(
            &format!("{}{}", short_hash(seed), extension),
            max_len,
        );
    }

    let mut normalized_stem =
        normalize_component(Path::new(stem), max_len - extension_len, seed);
    if is_reserved_name(&normalized_stem) {
        normalized_stem = format!("_{normalized_stem}");
    }

    let mut filename = format!("{normalized_stem}{extension}");
    if is_reserved_name(&filename) {
        filename = format!("_{filename}");
    }

    if component_len(&filename) > max_len {
        filename = truncate_component(&filename, max_len);
    }

    filename
}

pub fn normalize_path(
    path: &Path,
    can_truncate_parent_folder: bool,
    seed: &str,
    max_total_path_len: usize,
) -> PathBuf {
    let mut parent = path.parent().map(Path::to_path_buf);
    let mut directory = parent
        .as_ref()
        .and_then(|p| p.file_name().map(|p| Path::new(p).to_path_buf()));
    let file_stem = path.file_stem();
    let extension = path.extension();

    let mut normalized_file_name = if let Some(file_stem) = file_stem
        && let Some(extension) = extension
    {
        Some(
            normalize_filename(file_stem, extension, MAX_COMPONENT_LEN, seed)
                .into(),
        )
    } else {
        path.file_name().map(ToOwned::to_owned)
    };
    let mut normalized_path = path.to_path_buf();

    if can_truncate_parent_folder && let Some(dir) = directory {
        let normalized_directory = normalize_component(
            &dir,
            MAX_COMPONENT_LEN,
            &dir.to_string_lossy(),
        );
        directory = Some(normalized_directory.clone().into());
        parent = parent.as_mut().map(|p| {
            p.pop();
            p.join(normalized_directory)
        });

        if let Some(ref file_name) = normalized_file_name {
            normalized_path = parent.clone().map_or_else(
                || file_name.clone().into(),
                |p| p.join(file_name),
            );
        } else {
            normalized_path = parent.clone().unwrap_or_default();
        }
    }

    if let Some(ref file_name) = normalized_file_name {
        normalized_path = parent
            .as_ref()
            .map_or_else(|| file_name.clone().into(), |p| p.join(file_name));
    }

    if path_len(path) > max_total_path_len {
        if can_truncate_parent_folder {
            let available_dir_len = max_total_path_len
                .saturating_sub(parent.as_ref().map_or(0, |p| path_len(p)))
                .saturating_sub(2)
                .saturating_sub(
                    normalized_file_name.as_ref().map_or(0, |f| os_str_len(f)),
                )
                .clamp(1, MAX_COMPONENT_LEN);

            if let Some(directory_path) = directory {
                let truncated_directory_path = normalize_component(
                    &directory_path,
                    available_dir_len,
                    &directory_path.to_string_lossy(),
                );

                parent = parent.as_mut().map(|p| {
                    p.pop();
                    p.join(truncated_directory_path)
                });
                if let Some(ref file_name) = normalized_file_name {
                    normalized_path = parent.clone().map_or_else(
                        || file_name.clone().into(),
                        |p| p.join(file_name),
                    );
                } else {
                    normalized_path = parent.clone().unwrap_or_default();
                }
            }
        }

        if path_len(&normalized_path) > max_total_path_len {
            let available_filename_len = max_total_path_len
                .saturating_sub(parent.as_ref().map_or(0, |p| path_len(p)))
                .saturating_sub(1)
                .clamp(1, MAX_COMPONENT_LEN);
            if let Some(file_stem) = file_stem
                && let Some(extension) = extension
            {
                normalized_file_name = Some(
                    normalize_filename(
                        file_stem,
                        extension,
                        available_filename_len,
                        seed,
                    )
                    .into(),
                );

                if let Some(ref file_name) = normalized_file_name {
                    normalized_path = parent.map_or_else(
                        || file_name.clone().into(),
                        |p| p.join(file_name),
                    );
                }
            }
        }
    }

    normalized_path
}

pub fn sanitize_path(path: &str) -> String {
    if cfg!(target_os = "windows") {
        path.replace(['<', '>', ':', '"', '/', '\\', '|', '?', '*'], "_")
    } else {
        path.replace(['/'], "_")
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::OsString, path::PathBuf};

    use super::*;

    #[test]
    fn normalize_filename_sanitizes_separators() {
        let normalized = normalize_filename(
            &OsString::from("AC/DC"),
            &OsString::from("flac"),
            MAX_COMPONENT_LEN,
            "seed",
        );

        assert_eq!(normalized, "AC_DC.flac");
    }

    #[test]
    fn normalize_path_preserves_extension_with_limit() {
        let stem = "a".repeat(MAX_COMPONENT_LEN * 2);
        let path = PathBuf::from(stem).with_extension("flac");
        let normalized =
            normalize_path(&path, false, "seed", MAX_TOTAL_PATH_LEN);

        dbg!(&normalized);

        assert!(path_len(&normalized) <= MAX_COMPONENT_LEN);
        assert!(normalized.to_string_lossy().ends_with(".flac"));
    }

    #[test]
    fn normalize_path_truncates_overlong_paths() {
        let parent = "p".repeat(MAX_TOTAL_PATH_LEN + 50);
        let file_name = format!("{}.mp3", "n".repeat(MAX_COMPONENT_LEN + 50));
        let path = PathBuf::from(parent).join(file_name);

        let normalized =
            normalize_path(&path, true, "track-id", MAX_TOTAL_PATH_LEN);

        assert!(path_len(&normalized) <= MAX_TOTAL_PATH_LEN);

        if let Some(file_name) = normalized.file_name() {
            let file_name = file_name.to_string_lossy();
            assert!(component_len(&file_name) <= MAX_COMPONENT_LEN);
        }
    }

    #[test]
    fn normalize_paths() {
        struct TestCase {
            directory: PathBuf,
            filename: PathBuf,
            extension: OsString,
            can_truncate_parent_folder: bool,
            expected: PathBuf,
        }
        let cases = [
            TestCase {
                directory: PathBuf::from("."),
                filename: PathBuf::from(""),
                extension: OsString::new(),
                can_truncate_parent_folder: false,
                expected: PathBuf::from("."),
            },
            TestCase {
                directory: PathBuf::from("/"),
                filename: PathBuf::from(""),
                extension: OsString::new(),
                can_truncate_parent_folder: false,
                expected: PathBuf::from("/"),
            },
            TestCase {
                directory: PathBuf::from(".."),
                filename: PathBuf::from(""),
                extension: OsString::new(),
                can_truncate_parent_folder: false,
                expected: PathBuf::from(".."),
            },
            TestCase {
                directory: PathBuf::from("./AC_DC"),
                filename: PathBuf::from("AC_DC"),
                extension: OsString::from("mp3"),
                can_truncate_parent_folder: true,
                expected: PathBuf::from("./AC_DC/AC_DC.mp3"),
            },
            TestCase {
                directory: PathBuf::from("tmp/AC_DC"),
                filename: PathBuf::from("AC_DC"),
                extension: OsString::from("mp3"),
                can_truncate_parent_folder: true,
                expected: PathBuf::from("tmp/AC_DC/AC_DC.mp3"),
            },
            TestCase {
                directory: PathBuf::from("tmp/AC_DC"),
                filename: PathBuf::from("AC_DC".repeat(MAX_COMPONENT_LEN)),
                extension: OsString::from("mp3"),
                can_truncate_parent_folder: true,
                #[cfg(not(target_os = "windows"))]
                expected: PathBuf::from(
                    "tmp/AC_DC/AC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC~5045bcac.mp3",
                ),
                #[cfg(target_os = "windows")]
                expected: PathBuf::from(
                    "tmp\\~\\AC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCAC_DCA~5045bcac.mp3",
                ),
            },
        ];
        for case in cases {
            let normalized = normalize_path(
                &case
                    .directory
                    .join(case.filename)
                    .with_extension(case.extension),
                case.can_truncate_parent_folder,
                "seed",
                MAX_TOTAL_PATH_LEN,
            );

            assert!(path_len(&normalized) <= MAX_TOTAL_PATH_LEN);
            assert_eq!(normalized, case.expected);
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn normalize_path_keeps_normalized_filename_after_dir_truncate() {
        let parent = "p".repeat(MAX_TOTAL_PATH_LEN - 4);
        let path = PathBuf::from(parent).join("CON.mp3");

        let normalized =
            normalize_path(&path, true, "track-id", MAX_TOTAL_PATH_LEN);
        let file_name = normalized
            .file_name()
            .expect("normalized path must contain filename")
            .to_string_lossy()
            .into_owned();

        assert_eq!(file_name, "_CON.mp3");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn normalize_path_preserves_directory_with_long_path_limit() {
        // A deep base directory leaves almost no budget for the album folder
        // under the conservative MAX_PATH limit, so it collapses to a bare "~".
        // With long paths enabled the folder name is kept intact.
        let base = "b".repeat(MAX_TOTAL_PATH_LEN - 10);
        let directory = "Various Artists Compilation";
        let path = PathBuf::from(&base).join(directory).join("01 - Track.mp3");

        let conservative =
            normalize_path(&path, true, "seed", MAX_TOTAL_PATH_LEN);
        let conservative_dir = conservative
            .parent()
            .and_then(Path::file_name)
            .expect("path must contain a directory")
            .to_string_lossy()
            .into_owned();
        assert_eq!(conservative_dir, "~");

        let long =
            normalize_path(&path, true, "seed", LONG_MAX_TOTAL_PATH_LEN);
        let long_dir = long
            .parent()
            .and_then(Path::file_name)
            .expect("path must contain a directory")
            .to_string_lossy()
            .into_owned();
        assert_eq!(long_dir, directory);
    }
}
