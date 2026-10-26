//! Package-manager drivers as static data, plus detection and queries.
//!
//! Adding a manager means adding one `Pm` entry: no trait objects, no
//! factories, no per-manager code. Every command uses absolute paths.

use std::collections::BTreeSet;

use crate::parse;
use crate::sys;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Xbps,
    Pacman,
    Apt,
    Apk,
    Dnf,
    Dpkg,
    Rpm,
}

#[derive(Clone, Copy, Debug)]
pub struct Cmd {
    pub bins: &'static [&'static str],
    pub args: &'static [&'static str],
    /// Flags that suppress the manager's own confirmation, appended only in
    /// fast mode (`-y` / `y` / `s`). Interactive mode keeps the native
    /// transaction view, conflict resolution and virtual-provider menu.
    pub yes: &'static [&'static str],
    /// Text written to the child's stdin; used for `pacman -Scc`'s two prompts.
    pub stdin: Option<&'static str>,
}

const fn cmd(bins: &'static [&'static str], args: &'static [&'static str]) -> Cmd {
    Cmd {
        bins,
        args,
        yes: &[],
        stdin: None,
    }
}

const fn cmd_yes(
    bins: &'static [&'static str],
    args: &'static [&'static str],
    yes: &'static [&'static str],
) -> Cmd {
    Cmd {
        bins,
        args,
        yes,
        stdin: None,
    }
}

const fn cmd_in(
    bins: &'static [&'static str],
    args: &'static [&'static str],
    stdin: &'static str,
) -> Cmd {
    Cmd {
        bins,
        args,
        yes: &[],
        stdin: Some(stdin),
    }
}

#[derive(Debug)]
pub struct Pm {
    pub kind: Kind,
    pub name: &'static str,
    pub detect: &'static [&'static str],
    /// Whether the native tool asks its own confirmation. When false, the
    /// wrapper must ask; when true, `Auto::Ask` delegates to the native prompt.
    pub native_prompt: bool,
    pub install: &'static [Cmd],
    pub remove: &'static [Cmd],
    pub purge: &'static [Cmd],
    pub update: &'static [Cmd],
    pub autoclean: &'static [Cmd],
    pub clean: &'static [Cmd],
    pub orphan_query: &'static [Cmd],
    pub orphan_remove: &'static [Cmd],
    pub local_info: &'static [Cmd],
    pub remote_info: &'static [Cmd],
    pub search: &'static [Cmd],
    pub list_all: &'static [Cmd],
    pub list_manual: &'static [Cmd],
}

const PACMAN: &str = "/usr/bin/pacman";
const DPKG_QUERY: &str = "/usr/bin/dpkg-query";
const APT_GET: &str = "/usr/bin/apt-get";
const RPM: &str = "/usr/bin/rpm";
const DNF: &str = "/usr/bin/dnf";
const APK: &str = "/sbin/apk";

const XBPS: Pm = Pm {
    kind: Kind::Xbps,
    name: "xbps",
    native_prompt: true,
    detect: &[
        "/usr/bin/xbps-install",
        "/usr/bin/xbps-query",
        "/usr/bin/xbps-remove",
    ],
    install: &[cmd_yes(&["/usr/bin/xbps-install"], &["-S"], &["-y"])],
    remove: &[cmd_yes(&["/usr/bin/xbps-remove"], &[], &["-y"])],
    purge: &[cmd_yes(&["/usr/bin/xbps-remove"], &["-R"], &["-y"])],
    update: &[cmd_yes(&["/usr/bin/xbps-install"], &["-S", "-u"], &["-y"])],
    autoclean: &[cmd(&["/usr/bin/xbps-remove"], &["-O", "-y"])],
    clean: &[cmd_yes(&["/usr/bin/xbps-remove"], &["-O", "-o"], &["-y"])],
    orphan_query: &[],
    orphan_remove: &[],
    local_info: &[cmd(&["/usr/bin/xbps-query"], &[])],
    remote_info: &[cmd(&["/usr/bin/xbps-query"], &["-R"])],
    search: &[cmd(&["/usr/bin/xbps-query"], &["-Rs"])],
    list_all: &[cmd(&["/usr/bin/xbps-query"], &["-l"])],
    list_manual: &[cmd(&["/usr/bin/xbps-query"], &["-m"])],
};

const PACMAN_DRIVER: Pm = Pm {
    kind: Kind::Pacman,
    name: "pacman",
    native_prompt: true,
    detect: &[PACMAN],
    install: &[cmd_yes(&[PACMAN], &["-S", "--needed"], &["--noconfirm"])],
    remove: &[cmd_yes(&[PACMAN], &["-R"], &["--noconfirm"])],
    purge: &[cmd_yes(&[PACMAN], &["-Rns"], &["--noconfirm"])],
    update: &[cmd_yes(&[PACMAN], &["-Syu"], &["--noconfirm"])],
    // -Sc keeps the installed versions; -Scc wipes the cache after two prompts.
    autoclean: &[cmd(&[PACMAN], &["-Sc", "--noconfirm"])],
    clean: &[cmd_in(&[PACMAN], &["-Scc"], "y\ny\n")],
    orphan_query: &[cmd(&[PACMAN], &["-Qtdq"])],
    orphan_remove: &[cmd_yes(&[PACMAN], &["-Rns"], &["--noconfirm"])],
    local_info: &[cmd(&[PACMAN], &["-Qi"])],
    remote_info: &[cmd(&[PACMAN], &["-Si"])],
    // -Ss gives name, version and description in one spawn; -Ssq would drop them.
    search: &[cmd(&[PACMAN], &["-Ss"])],
    list_all: &[cmd(&[PACMAN], &["-Qq"])],
    list_manual: &[cmd(&[PACMAN], &["-Qqe"])],
};

const APT: Pm = Pm {
    kind: Kind::Apt,
    name: "apt",
    native_prompt: true,
    detect: &[
        APT_GET,
        "/usr/bin/apt-cache",
        "/usr/bin/apt-mark",
        DPKG_QUERY,
    ],
    install: &[cmd_yes(&[APT_GET], &["install"], &["-y"])],
    remove: &[cmd_yes(&[APT_GET], &["remove"], &["-y"])],
    purge: &[cmd_yes(&[APT_GET], &["purge", "--autoremove"], &["-y"])],
    update: &[
        cmd(&[APT_GET], &["update"]),
        cmd_yes(&[APT_GET], &["full-upgrade"], &["-y"]),
    ],
    autoclean: &[cmd(&[APT_GET], &["autoclean"])],
    clean: &[
        cmd_yes(&[APT_GET], &["autoremove", "--purge"], &["-y"]),
        cmd(&[APT_GET], &["clean"]),
    ],
    orphan_query: &[],
    orphan_remove: &[],
    local_info: &[cmd(
        &[DPKG_QUERY],
        &["-W", "-f=${Version}\\n${Description}\\n"],
    )],
    remote_info: &[cmd(&["/usr/bin/apt-cache"], &["show"])],
    search: &[cmd(&["/usr/bin/apt-cache"], &["search"])],
    list_all: &[cmd(
        &[DPKG_QUERY, "/bin/dpkg-query"],
        &["-W", "-f=${Package}\\n"],
    )],
    list_manual: &[cmd(&["/usr/bin/apt-mark"], &["showmanual"])],
};

const APK_DRIVER: Pm = Pm {
    kind: Kind::Apk,
    name: "apk",
    native_prompt: false,
    detect: &[APK, "/usr/bin/apk", "/usr/sbin/apk"],
    install: &[cmd(&[APK, "/usr/bin/apk"], &["add"])],
    remove: &[cmd(&[APK, "/usr/bin/apk"], &["del"])],
    // --purge is a global option: keep it before the applet for both apk 2.x/3.x.
    purge: &[cmd(&[APK, "/usr/bin/apk"], &["--purge", "del"])],
    update: &[
        cmd(&[APK, "/usr/bin/apk"], &["update"]),
        cmd(&[APK, "/usr/bin/apk"], &["upgrade"]),
    ],
    autoclean: &[cmd(&[APK, "/usr/bin/apk"], &["cache", "clean"])],
    clean: &[cmd(&[APK, "/usr/bin/apk"], &["cache", "purge"])],
    orphan_query: &[],
    orphan_remove: &[],
    local_info: &[cmd(&[APK, "/usr/bin/apk"], &["info", "-d"])],
    // Two -v: apk prints name-version at one, description at two.
    remote_info: &[cmd(&[APK, "/usr/bin/apk"], &["search", "-x", "-vv"])],
    search: &[cmd(&[APK, "/usr/bin/apk"], &["search", "-vv"])],
    // apk lists are read straight from /etc/apk/world and the installed database.
    list_all: &[],
    list_manual: &[],
};

const DNF_DRIVER: Pm = Pm {
    kind: Kind::Dnf,
    name: "dnf",
    native_prompt: true,
    detect: &[DNF, RPM],
    install: &[cmd_yes(&[DNF], &["install"], &["-y"])],
    remove: &[cmd_yes(&[DNF], &["remove"], &["-y"])],
    // dnf removes package-owned config files; there is no separate purge.
    purge: &[cmd_yes(&[DNF], &["remove"], &["-y"])],
    update: &[cmd_yes(&[DNF], &["upgrade", "--refresh"], &["-y"])],
    autoclean: &[cmd(&[DNF], &["clean", "packages"])],
    clean: &[
        cmd_yes(&[DNF], &["autoremove"], &["-y"]),
        cmd(&[DNF], &["clean", "all"]),
    ],
    orphan_query: &[],
    orphan_remove: &[],
    local_info: &[cmd(
        &[RPM],
        &["-q", "--qf", "%{VERSION}-%{RELEASE}\\n%{SUMMARY}\\n"],
    )],
    remote_info: &[cmd(&[DNF], &["info"])],
    search: &[cmd(&[DNF], &["-q", "list", "--available"])],
    list_all: &[cmd(&[RPM], &["-qa", "--qf", "%{NAME}\\n"])],
    list_manual: &[cmd(
        &[DNF],
        &[
            "repoquery",
            "--userinstalled",
            "--queryformat",
            "%{name}\\n",
        ],
    )],
};

const DNF5_DRIVER: Pm = Pm {
    kind: Kind::Dnf,
    name: "dnf5",
    native_prompt: true,
    detect: &["/usr/bin/dnf5", RPM],
    install: &[cmd_yes(&["/usr/bin/dnf5"], &["install"], &["-y"])],
    remove: &[cmd_yes(&["/usr/bin/dnf5"], &["remove"], &["-y"])],
    purge: &[cmd_yes(&["/usr/bin/dnf5"], &["remove"], &["-y"])],
    update: &[cmd_yes(
        &["/usr/bin/dnf5"],
        &["upgrade", "--refresh"],
        &["-y"],
    )],
    autoclean: &[cmd(&["/usr/bin/dnf5"], &["clean", "packages"])],
    clean: &[
        cmd_yes(&["/usr/bin/dnf5"], &["autoremove"], &["-y"]),
        cmd(&["/usr/bin/dnf5"], &["clean", "all"]),
    ],
    orphan_query: &[],
    orphan_remove: &[],
    local_info: &[cmd(
        &[RPM],
        &["-q", "--qf", "%{VERSION}-%{RELEASE}\\n%{SUMMARY}\\n"],
    )],
    remote_info: &[cmd(&["/usr/bin/dnf5"], &["info"])],
    search: &[cmd(&["/usr/bin/dnf5"], &["-q", "list", "--available"])],
    list_all: &[cmd(&[RPM], &["-qa", "--qf", "%{NAME}\\n"])],
    list_manual: &[cmd(
        &["/usr/bin/dnf5"],
        &[
            "repoquery",
            "--userinstalled",
            "--queryformat",
            "%{name}\\n",
        ],
    )],
};

const DPKG: Pm = Pm {
    kind: Kind::Dpkg,
    name: "dpkg",
    native_prompt: false,
    detect: &["/usr/bin/dpkg", DPKG_QUERY],
    install: &[],
    remove: &[cmd(&["/usr/bin/dpkg"], &["--remove"])],
    purge: &[cmd(&["/usr/bin/dpkg"], &["--purge"])],
    update: &[],
    autoclean: &[],
    clean: &[],
    orphan_query: &[],
    orphan_remove: &[],
    local_info: &[cmd(
        &[DPKG_QUERY],
        &["-W", "-f=${Version}\\n${Description}\\n"],
    )],
    remote_info: &[],
    search: &[],
    list_all: &[cmd(&[DPKG_QUERY], &["-W", "-f=${Package}\\n"])],
    list_manual: &[],
};

const RPM_DRIVER: Pm = Pm {
    kind: Kind::Rpm,
    name: "rpm",
    native_prompt: false,
    detect: &[RPM],
    install: &[],
    remove: &[cmd(&[RPM], &["-e"])],
    purge: &[cmd(&[RPM], &["-e"])],
    update: &[],
    autoclean: &[],
    clean: &[],
    orphan_query: &[],
    orphan_remove: &[],
    local_info: &[cmd(
        &[RPM],
        &["-q", "--qf", "%{VERSION}-%{RELEASE}\\n%{SUMMARY}\\n"],
    )],
    remote_info: &[],
    search: &[],
    list_all: &[cmd(&[RPM], &["-qa", "--qf", "%{NAME}\\n"])],
    list_manual: &[],
};

/// Detection order: high-level repositories first, low-level packages last.
static DRIVERS: &[Pm] = &[
    XBPS,
    APK_DRIVER,
    PACMAN_DRIVER,
    APT,
    DNF_DRIVER,
    DNF5_DRIVER,
    DPKG,
    RPM_DRIVER,
];

#[derive(Clone, Debug)]
pub struct Meta {
    pub name: String,
    pub version: String,
    pub description: String,
}

/// First executable candidate, falling back to the first declared path so the
/// eventual spawn error names the right binary.
pub fn resolve(command: &Cmd) -> Option<&'static str> {
    command
        .bins
        .iter()
        .copied()
        .find(|path| sys::is_executable(path))
        .or_else(|| command.bins.first().copied())
}

/// Run a list of commands with `extra` appended, concatenating their stdout.
pub fn capture(commands: &[Cmd], extra: &[&str]) -> Option<String> {
    if commands.is_empty() {
        return None;
    }
    let mut output = String::new();
    for command in commands {
        let bin = resolve(command)?;
        let mut parts: Vec<&str> = Vec::with_capacity(1 + command.args.len() + extra.len());
        parts.push(bin);
        parts.extend_from_slice(command.args);
        parts.extend_from_slice(extra);
        output.push_str(&sys::capture(&parts)?);
        output.push('\n');
    }
    Some(output)
}

pub fn local(pm: &Pm, name: &str) -> Option<Meta> {
    let text = capture(pm.local_info, &[name])?;
    let (version, description) = parse::local(pm.kind, name, &text)?;
    Some(Meta {
        name: name.to_owned(),
        version,
        description,
    })
}

pub fn remote(pm: &Pm, name: &str) -> Option<Meta> {
    let text = capture(pm.remote_info, &[name])?;
    let (version, description) = parse::remote(pm.kind, name, &text)?;
    Some(Meta {
        name: name.to_owned(),
        version,
        description,
    })
}

pub fn search(pm: &Pm, query: &str) -> Vec<Meta> {
    capture(pm.search, &[query]).map_or_else(Vec::new, |text| parse::search(pm.kind, &text))
}

pub fn search_raw(pm: &Pm, query: &str) -> Option<String> {
    capture(pm.search, &[query])
}

/// `(system, user)` package sets, sorted and deduplicated.
pub fn installed(pm: &Pm) -> (Vec<String>, Vec<String>) {
    let all = if pm.kind == Kind::Apk {
        apk_db()
    } else {
        capture_names(pm.list_all, pm.kind)
    };
    let manual = if pm.kind == Kind::Apk {
        apk_world()
    } else {
        capture_names(pm.list_manual, pm.kind)
    };
    let user: Vec<String> = manual.intersection(&all).cloned().collect();
    let system: Vec<String> = all.difference(&manual).cloned().collect();
    (system, user)
}

fn capture_names(commands: &[Cmd], kind: Kind) -> BTreeSet<String> {
    capture(commands, &[]).map_or_else(BTreeSet::new, |text| {
        parse::names(kind, &text).into_iter().collect()
    })
}

fn apk_db() -> BTreeSet<String> {
    let Ok(text) = std::fs::read_to_string("/lib/apk/db/installed") else {
        return BTreeSet::new();
    };
    text.lines()
        .filter_map(|line| line.strip_prefix("P:"))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}

fn apk_world() -> BTreeSet<String> {
    std::fs::read_to_string("/etc/apk/world")
        .map_or_else(|_| BTreeSet::new(), |text| world_names(&text))
}

/// `/etc/apk/world` holds plain names with optional pins: `pkg=1.2`, `pkg@tag`,
/// `pkg>1`. Strip from the first pin character.
fn world_names(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            line.split(['=', '@', '<', '>', '~'])
                .next()
                .unwrap_or(line)
                .to_owned()
        })
        .collect()
}

/// In-memory only: a handful of `access(X_OK)` calls cost microseconds, so the
/// disk cache bought nothing and created root-owned files under a user's HOME.
pub fn detect() -> Option<&'static Pm> {
    DRIVERS
        .iter()
        .find(|pm| pm.detect.iter().all(|path| sys::is_executable(path)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_commands(pm: &Pm) -> impl Iterator<Item = &Cmd> {
        [
            pm.install,
            pm.remove,
            pm.purge,
            pm.update,
            pm.autoclean,
            pm.clean,
            pm.orphan_query,
            pm.orphan_remove,
            pm.local_info,
            pm.remote_info,
            pm.search,
            pm.list_all,
            pm.list_manual,
        ]
        .into_iter()
        .flatten()
    }

    /// The whole safety model rests on never resolving a name through PATH and
    /// never calling a binary named like the wrapper itself.
    #[test]
    fn every_binary_is_absolute_and_not_ours() {
        for pm in DRIVERS {
            for command in all_commands(pm) {
                for bin in command.bins {
                    assert!(bin.starts_with('/'), "{}: {bin}", pm.name);
                    assert!(!bin.ends_with("/ryclop"), "{}: {bin}", pm.name);
                    assert!(!bin.ends_with("/pkg"), "{}: {bin}", pm.name);
                }
            }
        }
    }

    #[test]
    fn every_driver_has_a_detection_path() {
        for pm in DRIVERS {
            assert!(!pm.detect.is_empty(), "{}", pm.name);
        }
    }

    #[test]
    fn apk_world_strips_pins_and_comments() {
        let world = "# base\nmusl=1.2.4-r2\nvim@edge\ncurl>8\n";
        let names = world_names(world);
        assert_eq!(names.len(), 3);
        assert!(names.contains("musl"));
        assert!(names.contains("vim"));
        assert!(names.contains("curl"));
    }
}
