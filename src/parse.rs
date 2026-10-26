//! Parsers for package-manager output. Every subprocess runs with `LC_ALL=C`,
//! so field names and separators here are deterministic in any user locale.

use crate::pm::{Kind, Meta};

/// XBPS version invariant: `name-1.2.3_1`. Requiring the `_` keeps a clean
/// name such as `llvm-18` intact.
fn xbps_parts(token: &str) -> Option<(&str, &str)> {
    let (name, version) = token.rsplit_once('-')?;
    (version.starts_with(|c: char| c.is_ascii_digit()) && version.contains('_'))
        .then_some((name, version))
}

/// APK version invariant: `name-1.2.3-r0`. Requiring the `-rN` revision keeps
/// a clean name such as `gtk-4` intact.
fn apk_parts(token: &str) -> Option<(&str, &str)> {
    let (head, revision) = token.rsplit_once('-')?;
    let digits = revision.strip_prefix('r')?;
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let (name, version) = head.rsplit_once('-')?;
    if !version.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    // Keep the revision in the displayed version: `9.1.0-r0`, not `9.1.0`.
    Some((name, &token[name.len() + 1..]))
}

/// Strip the version from an XBPS or APK `name-version` token, if and only if
/// it carries that manager's version marker. Never guesses.
pub fn strip_version(token: &str) -> &str {
    xbps_parts(token)
        .or_else(|| apk_parts(token))
        .map_or(token, |(name, _)| name)
}

fn field<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if name.trim().eq_ignore_ascii_case(key) && !value.trim().is_empty() {
            Some(value.trim())
        } else {
            None
        }
    })
}

fn first_two_lines(text: &str) -> Option<(String, String)> {
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let version = lines.next()?.to_owned();
    let description = lines.next().unwrap_or("").to_owned();
    Some((version, description))
}

fn strip_name_version<'a>(name: &str, token: &'a str) -> Option<&'a str> {
    token.strip_prefix(name)?.strip_prefix('-')
}

fn apk_pair(name: &str, text: &str) -> Option<(String, String)> {
    let mut lines = text.lines().map(str::trim);
    let header = lines.next()?;
    let version = strip_name_version(name, header.split_whitespace().next()?)?;
    let description = lines.next().unwrap_or("").to_owned();
    Some((version.to_owned(), description))
}

/// Version and short description of an installed package.
pub fn local(kind: Kind, name: &str, text: &str) -> Option<(String, String)> {
    match kind {
        Kind::Xbps => Some((
            field(text, "pkgver")?.to_owned(),
            field(text, "short_desc")?.to_owned(),
        )),
        Kind::Pacman => Some((
            field(text, "Version")?.to_owned(),
            field(text, "Description")?.to_owned(),
        )),
        Kind::Apt | Kind::Dpkg | Kind::Dnf | Kind::Rpm => first_two_lines(text),
        Kind::Apk => apk_pair(name, text),
    }
}

/// Version and short description of a repository package.
pub fn remote(kind: Kind, name: &str, text: &str) -> Option<(String, String)> {
    match kind {
        Kind::Xbps => Some((
            field(text, "pkgver")?.to_owned(),
            field(text, "short_desc")?.to_owned(),
        )),
        Kind::Pacman | Kind::Apt => Some((
            field(text, "Version")?.to_owned(),
            field(text, "Description")?.to_owned(),
        )),
        Kind::Dnf => Some((
            field(text, "Version")?.to_owned(),
            field(text, "Summary")?.to_owned(),
        )),
        Kind::Apk => {
            let (left, right) = text.lines().next()?.split_once(" - ")?;
            let version = strip_name_version(name, left.trim())?;
            Some((version.to_owned(), right.trim().to_owned()))
        }
        Kind::Dpkg | Kind::Rpm => None,
    }
}

/// Package summaries from a native search command. Every manager but dnf gives
/// name, version and description in one spawn; keeping them avoids both a blind
/// menu and an extra `remote_info` fork for a single hit.
pub fn search(kind: Kind, text: &str) -> Vec<Meta> {
    match kind {
        Kind::Pacman => pacman_search(text),
        Kind::Xbps => xbps_search(text),
        Kind::Apt => apt_search(text),
        Kind::Apk => apk_search(text),
        Kind::Dnf => dnf_search(text),
        Kind::Dpkg | Kind::Rpm => Vec::new(),
    }
}

fn meta(name: &str, version: &str, description: &str) -> Meta {
    Meta {
        name: name.to_owned(),
        version: version.to_owned(),
        description: description.to_owned(),
    }
}

/// `pacman -Ss`: `repo/name version` followed by an indented description line.
fn pacman_search(text: &str) -> Vec<Meta> {
    let mut hits: Vec<Meta> = Vec::new();
    let mut current: Option<Meta> = None;
    for raw in text.lines() {
        if raw.starts_with(char::is_whitespace) {
            if let Some(hit) = current.as_mut() {
                raw.trim().clone_into(&mut hit.description);
            }
            continue;
        }
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(hit) = current.take() {
            hits.push(hit);
        }
        let mut fields = line.split_whitespace();
        let Some(path) = fields.next() else {
            continue;
        };
        let name = path.rsplit_once('/').map_or(path, |(_, name)| name);
        current = Some(meta(name, fields.next().unwrap_or(""), ""));
    }
    if let Some(hit) = current {
        hits.push(hit);
    }
    hits
}

/// `xbps-query -Rs`: `[*] name-version description`.
fn xbps_search(text: &str) -> Vec<Meta> {
    text.lines()
        .filter_map(|raw| {
            let line = raw.trim();
            let rest = line.split_once(' ').map(|(_, rest)| rest.trim_start())?;
            // Skip the [*]/[-] state column, then split token from description.
            let (token, description) = rest
                .split_once(' ')
                .map_or((rest, ""), |(token, description)| {
                    (token, description.trim())
                });
            let (name, version) = xbps_parts(token)?;
            Some(meta(name, version, description))
        })
        .collect()
}

/// `apt-cache search`: `name - description` (no version in the output).
fn apt_search(text: &str) -> Vec<Meta> {
    text.lines()
        .filter_map(|raw| {
            let line = raw.trim();
            let (left, description) = line.split_once(" - ")?;
            let name = left.split_whitespace().next()?;
            Some(meta(name, "", description.trim()))
        })
        .collect()
}

/// `apk search -vv`: `name-version - description`.
fn apk_search(text: &str) -> Vec<Meta> {
    text.lines()
        .filter_map(|raw| {
            let line = raw.trim();
            if line.is_empty() {
                return None;
            }
            let (left, description) = line
                .split_once(" - ")
                .map_or((line, ""), |(left, description)| (left, description.trim()));
            let (name, version) = apk_parts(left.trim())?;
            Some(meta(name, version, description))
        })
        .collect()
}

/// `dnf list --available`: `name.arch version repo` (no description).
fn dnf_search(text: &str) -> Vec<Meta> {
    text.lines()
        .filter_map(|raw| {
            let mut fields = raw.split_whitespace();
            let first = fields.next()?;
            let (name, _) = first.rsplit_once('.')?;
            Some(meta(name, fields.next().unwrap_or(""), ""))
        })
        .collect()
}

/// Package names from a native list command.
pub fn names(kind: Kind, text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let first = fields.next()?;
            let token = if kind == Kind::Xbps && matches!(first, "ii" | "uu" | "un" | "hr") {
                fields.next().unwrap_or(first)
            } else {
                first
            };
            let name = if kind == Kind::Xbps {
                strip_version(token)
            } else {
                token
            };
            (!name.is_empty()).then(|| name.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names_of(hits: &[Meta]) -> Vec<&str> {
        hits.iter().map(|hit| hit.name.as_str()).collect()
    }

    #[test]
    fn strips_only_marked_versions() {
        assert_eq!(strip_version("vim-9.1.0_1"), "vim");
        assert_eq!(strip_version("webkit2gtk-4.1-2.44.0_1"), "webkit2gtk-4.1");
        assert_eq!(strip_version("vim-9.1.0-r0"), "vim");
        assert_eq!(strip_version("libfoo-bar-1.2-r0"), "libfoo-bar");
        // Clean names that happen to end in -<digit> must survive.
        assert_eq!(strip_version("llvm-18"), "llvm-18");
        assert_eq!(strip_version("gtk-4"), "gtk-4");
        assert_eq!(strip_version("font-misc-75dpi"), "font-misc-75dpi");
        assert_eq!(strip_version("pkg"), "pkg");
    }

    #[test]
    fn xbps_installed() {
        let out = "pkgver: 9.1.0_1\nshort_desc: Vi Improved\n";
        assert_eq!(
            local(Kind::Xbps, "vim", out),
            Some(("9.1.0_1".to_owned(), "Vi Improved".to_owned()))
        );
    }

    #[test]
    fn pacman_remote() {
        let out =
            "Name            : vim\nVersion         : 9.1.0-2\nDescription     : Vi Improved\n";
        assert_eq!(
            remote(Kind::Pacman, "vim", out),
            Some(("9.1.0-2".to_owned(), "Vi Improved".to_owned()))
        );
    }

    #[test]
    fn apt_local_is_two_lines() {
        let out = "2:9.1.0-1\nVi IMproved - enhanced vi editor\n Vi IMproved\n";
        assert_eq!(
            local(Kind::Apt, "vim", out),
            Some((
                "2:9.1.0-1".to_owned(),
                "Vi IMproved - enhanced vi editor".to_owned()
            ))
        );
    }

    #[test]
    fn apt_search_keeps_description() {
        let out = "vim - Vi IMproved\nvim-runtime - Runtime files\n";
        let hits = search(Kind::Apt, out);
        assert_eq!(names_of(&hits), ["vim", "vim-runtime"]);
        assert_eq!(hits[0].description, "Vi IMproved");
        assert_eq!(hits[0].version, "");
    }

    #[test]
    fn xbps_search_and_list() {
        let out = "[*] vim-9.1.0_1 Vi Improved\n[-] vim-tiny-9.1.0_1 Tiny\n";
        let hits = search(Kind::Xbps, out);
        assert_eq!(names_of(&hits), ["vim", "vim-tiny"]);
        assert_eq!(hits[0].version, "9.1.0_1");
        assert_eq!(hits[0].description, "Vi Improved");

        let listed = "ii vim-9.1.0_1 Vi Improved\nii musl-1.2.4_1 C library\n";
        assert_eq!(names(Kind::Xbps, listed), ["vim", "musl"]);
    }

    #[test]
    fn dnf_search_strips_arch() {
        let out = "Available Packages\nvim-enhanced.x86_64 2:9.1-1.fc41 fedora\n";
        let hits = search(Kind::Dnf, out);
        assert_eq!(names_of(&hits), ["vim-enhanced"]);
        assert_eq!(hits[0].version, "2:9.1-1.fc41");
    }

    #[test]
    fn pacman_ss_keeps_metadata() {
        let out =
            "extra/vim 9.1.0-2 [installed]\n    Vi Improved\ncore/zstd 1.5.6-1\n    Zstandard\n";
        let hits = search(Kind::Pacman, out);
        assert_eq!(names_of(&hits), ["vim", "zstd"]);
        assert_eq!(hits[0].version, "9.1.0-2");
        assert_eq!(hits[0].description, "Vi Improved");
        assert_eq!(hits[1].description, "Zstandard");
    }

    #[test]
    fn apk_remote_and_local() {
        let remote_out = "vim-9.1.0-r0 - Vi Improved\n";
        assert_eq!(
            remote(Kind::Apk, "vim", remote_out),
            Some(("9.1.0-r0".to_owned(), "Vi Improved".to_owned()))
        );

        let local_out = "vim-9.1.0-r0 description:\nVi Improved\n";
        assert_eq!(
            local(Kind::Apk, "vim", local_out),
            Some(("9.1.0-r0".to_owned(), "Vi Improved".to_owned()))
        );
    }

    #[test]
    fn apk_search_keeps_metadata() {
        let out = "vim-9.1.0-r0 - Vi Improved\n";
        let hits = search(Kind::Apk, out);
        assert_eq!(names_of(&hits), ["vim"]);
        assert_eq!(hits[0].version, "9.1.0-r0");
        assert_eq!(hits[0].description, "Vi Improved");
    }
}
