//! Workflows. Resolve everything without root first, then elevate once per
//! transaction and run the automatic, non-destructive cleanup.

use crate::cli::{Args, Auto, Op};
use crate::i18n::Messages;
use crate::pm::{self, Cmd, Pm};
use crate::sys::{self, RootError};
use crate::term;

pub fn run(args: &Args, msg: &Messages) -> Result<(), String> {
    let pm = pm::detect().ok_or_else(|| msg.no_manager.to_owned())?;
    match args.op {
        Op::Install => install(pm, args, msg),
        Op::Remove => remove(pm, args, msg, Mode::Remove),
        Op::Uninstall => remove(pm, args, msg, Mode::Purge),
        Op::Update => update(pm, args, msg),
        Op::Clean => clean(pm, args, msg),
        Op::List => {
            list(pm, msg);
            Ok(())
        }
        Op::Info => {
            info(pm, args, msg);
            Ok(())
        }
        Op::Search => {
            search(pm, args, msg);
            Ok(())
        }
    }
}

fn install(pm: &Pm, args: &Args, msg: &Messages) -> Result<(), String> {
    if pm.install.is_empty() {
        return Err(unsupported(pm, Op::Install, msg));
    }

    let mut queue: Vec<String> = Vec::new();
    for name in &args.packages {
        if let Some(meta) = pm::local(pm, name) {
            term::ok(&render(msg.already, &meta));
            continue;
        }
        if let Some(meta) = pm::remote(pm, name) {
            term::info(&render(msg.available, &meta));
            if confirm(pm, msg.install_confirm, args.auto) {
                queue.push(name.clone());
            }
            continue;
        }
        queue.extend(choose(pm, name, msg, args.auto));
    }

    if queue.is_empty() {
        return Ok(());
    }
    term::info(&render_items(msg.installing, &queue));
    root(pm.install, &refs(&queue), msg, args.auto)?;
    autoclean(pm, msg);
    term::ok(msg.done);
    Ok(())
}

/// Search fallback: 0 hits warn, 1 hit shows its metadata, many hits open a
/// numeric menu with the version and description the native search already
/// returned.
fn choose(pm: &Pm, query: &str, msg: &Messages, auto: Auto) -> Vec<String> {
    let hits = pm::search(pm, query);
    match hits.len() {
        0 => {
            term::warn(&render_items(msg.not_found, &[query.to_owned()]));
            Vec::new()
        }
        1 => {
            let hit = &hits[0];
            // apt-cache search has no version; ask the repo only for the gap.
            let meta = if hit.version.is_empty() || hit.description.is_empty() {
                pm::remote(pm, &hit.name).unwrap_or_else(|| hit.clone())
            } else {
                hit.clone()
            };
            term::info(&render(msg.available, &meta));
            if confirm(pm, msg.install_confirm, auto) {
                vec![meta.name]
            } else {
                Vec::new()
            }
        }
        count => {
            let shown = count.min(15);
            term::info(&render_items(msg.suggestions, &[query.to_owned()]));
            for (index, hit) in hits.iter().take(shown).enumerate() {
                term::info(&hit_line(index + 1, hit));
            }
            let prompt = msg.select.replace("{max}", &shown.to_string());
            term::select(&prompt, shown)
                .into_iter()
                .map(|index| hits[index].name.clone())
                .collect()
        }
    }
}

fn hit_line(index: usize, hit: &pm::Meta) -> String {
    match (hit.version.is_empty(), hit.description.is_empty()) {
        (true, true) => format!("  [{index}] {}", hit.name),
        (true, false) => format!("  [{index}] {} — {}", hit.name, hit.description),
        (false, true) => format!("  [{index}] {} ({})", hit.name, hit.version),
        (false, false) => format!(
            "  [{index}] {} ({}) — {}",
            hit.name, hit.version, hit.description
        ),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Remove,
    Purge,
}

fn remove(pm: &Pm, args: &Args, msg: &Messages, mode: Mode) -> Result<(), String> {
    let (op, commands, prompt, banner) = match mode {
        Mode::Remove => (Op::Remove, pm.remove, msg.remove_confirm, msg.removing),
        Mode::Purge => (Op::Uninstall, pm.purge, msg.purge_confirm, msg.purging),
    };
    if commands.is_empty() {
        return Err(unsupported(pm, op, msg));
    }

    let mut queue: Vec<String> = Vec::new();
    for name in &args.packages {
        if pm::local(pm, name).is_some() {
            queue.push(name.clone());
        } else {
            term::warn(&render_items(msg.not_found, std::slice::from_ref(name)));
            suggest(pm, name, msg);
        }
    }

    if queue.is_empty() {
        return Ok(());
    }
    if !confirm(pm, prompt, args.auto) {
        term::info(msg.cancelled);
        return Ok(());
    }

    term::info(&render_items(banner, &queue));
    root(commands, &refs(&queue), msg, args.auto)?;
    autoclean(pm, msg);
    term::ok(msg.done);
    Ok(())
}

fn suggest(pm: &Pm, query: &str, msg: &Messages) {
    let hits = pm::search(pm, query);
    if hits.is_empty() {
        return;
    }
    term::info(&render_items(msg.suggestions, &[query.to_owned()]));
    for (index, hit) in hits.iter().take(10).enumerate() {
        term::info(&hit_line(index + 1, hit));
    }
}

fn update(pm: &Pm, args: &Args, msg: &Messages) -> Result<(), String> {
    if pm.update.is_empty() {
        return Err(unsupported(pm, Op::Update, msg));
    }
    if !confirm(pm, msg.update_confirm, args.auto) {
        term::info(msg.cancelled);
        return Ok(());
    }
    term::info(msg.updating);
    root(pm.update, &[], msg, args.auto)?;
    autoclean(pm, msg);
    term::ok(msg.done);
    Ok(())
}

fn clean(pm: &Pm, args: &Args, msg: &Messages) -> Result<(), String> {
    if pm.clean.is_empty() {
        return Err(unsupported(pm, Op::Clean, msg));
    }
    if !confirm(pm, msg.clean_confirm, args.auto) {
        term::info(msg.cancelled);
        return Ok(());
    }
    term::info(msg.cleaning);
    root(pm.clean, &[], msg, args.auto)?;
    orphans(pm, msg, args.auto)?;
    term::ok(msg.done);
    Ok(())
}

/// Managers that expose orphan detection as a separate query (pacman).
fn orphans(pm: &Pm, msg: &Messages, auto: Auto) -> Result<(), String> {
    if pm.orphan_query.is_empty() || pm.orphan_remove.is_empty() {
        return Ok(());
    }
    let Some(text) = pm::capture(pm.orphan_query, &[]) else {
        // `pacman -Qtdq` exits 1 with empty stdout when there is nothing to
        // list; `capture` maps that to None, which is the "no orphans" case.
        return Ok(());
    };
    let names: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    if names.is_empty() {
        return Ok(());
    }
    let listed: Vec<String> = names.iter().map(|name| (*name).to_owned()).collect();
    term::info(&render_items(msg.orphans, &listed));
    root(pm.orphan_remove, &names, msg, auto)
}

/// Non-destructive cleanup after install/remove/update. Failure is reported,
/// never fatal: the transaction itself already succeeded.
fn autoclean(pm: &Pm, msg: &Messages) {
    if pm.autoclean.is_empty() {
        return;
    }
    // Always fast mode: post-transaction cleanup is non-destructive and must
    // never stop to ask after the transaction already succeeded.
    if let Err(err) = root(pm.autoclean, &[], msg, Auto::Yes) {
        term::warn(&err);
    }
}

fn root(commands: &[Cmd], extra: &[&str], msg: &Messages, auto: Auto) -> Result<(), String> {
    for command in commands {
        let bin = pm::resolve(command)
            .ok_or_else(|| format!("{}: {}", msg.error, command.bins.join(", ")))?;
        let parts = argv(bin, command, extra, auto);
        // In interactive mode the child keeps the TTY, so prompts it owns
        // (pacman -Scc) are answered by the user, not by a canned pipe.
        let input = if auto == Auto::Yes {
            command.stdin
        } else {
            None
        };
        // ponytail: one elevation per command; sudo caches, doas/run0 may ask
        // again. Batch under `sh -c` if prompt fatigue ever matters.
        sys::run_root(&parts, input).map_err(|err| root_error(err, msg))?;
    }
    Ok(())
}

/// In interactive mode, delegate to the manager's own prompt when it has one.
/// That is the prompt that shows the real transaction and resolves conflicts.
fn confirm(pm: &Pm, prompt: &str, auto: Auto) -> bool {
    if pm.native_prompt {
        true
    } else {
        term::confirm(prompt, auto)
    }
}

/// Build the final argv. Native autoconfirmation is appended only in fast mode:
/// in interactive mode the manager must see the transaction and ask.
fn argv<'a>(bin: &'a str, command: &'a Cmd, extra: &'a [&'a str], auto: Auto) -> Vec<&'a str> {
    let mut parts = Vec::with_capacity(1 + command.args.len() + command.yes.len() + extra.len());
    parts.push(bin);
    parts.extend_from_slice(command.args);
    if auto == Auto::Yes {
        parts.extend_from_slice(command.yes);
    }
    parts.extend_from_slice(extra);
    parts
}

fn root_error(err: RootError, msg: &Messages) -> String {
    match err {
        RootError::NoTool => msg.no_root_tool.to_owned(),
        RootError::Io(source) => format!("{}: {source}", msg.error),
        RootError::Failed(code) => msg.failed.replace("{code}", &code.to_string()),
    }
}

fn unsupported(pm: &Pm, op: Op, msg: &Messages) -> String {
    msg.unsupported
        .replace("{pm}", pm.name)
        .replace("{op}", op.label())
}

fn list(pm: &Pm, msg: &Messages) {
    let (system, user) = pm::installed(pm);
    term::info(&term::bold(msg.system_pkgs));
    for name in &system {
        term::info(name);
    }
    term::info("");
    term::info(&term::bold(msg.user_pkgs));
    for name in &user {
        term::info(name);
    }
}

fn info(pm: &Pm, args: &Args, msg: &Messages) {
    for name in &args.packages {
        match pm::local(pm, name).or_else(|| pm::remote(pm, name)) {
            Some(meta) => {
                term::info(&term::bold(&meta.name));
                term::info(&msg.version.replace("{version}", &meta.version));
                term::info(&meta.description);
            }
            None => term::warn(&render_items(msg.not_found, std::slice::from_ref(name))),
        }
    }
}

fn search(pm: &Pm, args: &Args, msg: &Messages) {
    for query in &args.packages {
        if args.packages.len() > 1 {
            term::info(&term::bold(query));
        }
        match pm::search_raw(pm, query) {
            Some(text) if !text.trim().is_empty() => term::info(text.trim_end()),
            _ => term::warn(&render_items(msg.no_results, std::slice::from_ref(query))),
        }
    }
}

fn render(template: &str, meta: &pm::Meta) -> String {
    template
        .replace("{name}", &meta.name)
        .replace("{version}", &meta.version)
        .replace("{desc}", &meta.description)
}

fn render_items(template: &str, items: &[String]) -> String {
    template.replace("{items}", &items.join(" "))
}

fn refs(items: &[String]) -> Vec<&str> {
    items.iter().map(String::as_str).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST: Cmd = Cmd {
        bins: &["/bin/true"],
        args: &["install"],
        yes: &["-y"],
        stdin: None,
    };

    #[test]
    fn fast_mode_appends_native_yes_and_packages_last() {
        assert_eq!(
            argv("/bin/true", &TEST, &["vim"], Auto::Yes),
            ["/bin/true", "install", "-y", "vim"]
        );
    }

    #[test]
    fn interactive_mode_leaves_the_native_prompt() {
        assert_eq!(
            argv("/bin/true", &TEST, &["vim"], Auto::Ask),
            ["/bin/true", "install", "vim"]
        );
    }
}
