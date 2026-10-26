//! Compile-time translations. Spanish and English, detected from the locale.

use std::env;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
}

#[derive(Debug)]
pub struct Messages {
    pub usage: &'static str,
    pub already: &'static str,
    pub available: &'static str,
    pub version: &'static str,
    pub install_confirm: &'static str,
    pub remove_confirm: &'static str,
    pub purge_confirm: &'static str,
    pub update_confirm: &'static str,
    pub clean_confirm: &'static str,
    pub select: &'static str,
    pub cancelled: &'static str,
    pub not_found: &'static str,
    pub suggestions: &'static str,
    pub installing: &'static str,
    pub removing: &'static str,
    pub purging: &'static str,
    pub updating: &'static str,
    pub cleaning: &'static str,
    pub orphans: &'static str,
    pub done: &'static str,
    pub unsupported: &'static str,
    pub no_manager: &'static str,
    pub no_root_tool: &'static str,
    pub failed: &'static str,
    pub error: &'static str,
    pub system_pkgs: &'static str,
    pub user_pkgs: &'static str,
    pub no_results: &'static str,
    pub unknown_flag: &'static str,
    pub missing_package: &'static str,
    pub unexpected_arg: &'static str,
    pub invalid_package: &'static str,
}

pub static ES: Messages = Messages {
    usage: "ryclop — envoltorio universal de gestores de paquetes

Uso: ryclop <paquete>...        instala (atajo de `ryclop i`)
     ryclop <op> [paquete]...

Ops:  i, install    u, update    r, remove    U, uninstall (purga)
      l, list       c, clean     I, info      s, search

Añade y/s al final de la op (iy, rs, us, cs...) o -y para no preguntar.
Usa `--` antes de un paquete que se llame igual que una op: ryclop i -- info
Opciones: -h, --help   -V, --version",
    already: "«{name}» {version} ya está instalado: {desc}",
    available: "«{name}» {version} — {desc}",
    version: "versión: {version}",
    install_confirm: "¿Instalar? [s/N]: ",
    remove_confirm: "¿Desinstalar? [s/N]: ",
    purge_confirm: "¿Purgar (paquete + configuración + huérfanos)? [s/N]: ",
    update_confirm: "¿Actualizar repositorios y sistema? [s/N]: ",
    clean_confirm: "¿Limpiar caché y huérfanos? [s/N]: ",
    select: "Elige [1-{max}] separados por espacio, o n para cancelar: ",
    cancelled: "Cancelado.",
    not_found: "Sin coincidencias para «{items}»",
    suggestions: "Sugerencias para «{items}»:",
    installing: "Instalando: {items}",
    removing: "Desinstalando: {items}",
    purging: "Purgando: {items}",
    updating: "Actualizando repositorios y sistema...",
    cleaning: "Limpiando caché y huérfanos...",
    orphans: "Huérfanos: {items}",
    done: "Listo.",
    unsupported: "{pm} no soporta la operación «{op}»",
    no_manager: "no se detectó ningún gestor de paquetes compatible",
    no_root_tool: "no hay forma de elevar privilegios (doas, sudo, run0, su)",
    failed: "el gestor terminó con código {code}",
    error: "error",
    system_pkgs: "=== SISTEMA / DEPENDENCIAS AUTOMÁTICAS ===",
    user_pkgs: "=== INSTALADOS EXPLÍCITAMENTE POR EL USUARIO ===",
    no_results: "Sin resultados para «{items}»",
    unknown_flag: "opción desconocida",
    missing_package: "falta el paquete para «{op}»",
    unexpected_arg: "argumento inesperado",
    invalid_package: "nombre de paquete inválido",
};

pub static EN: Messages = Messages {
    usage: "ryclop — universal package manager wrapper

Usage: ryclop <package>...        installs (shortcut for `ryclop i`)
       ryclop <op> [package]...

Ops:  i, install    u, update    r, remove    U, uninstall (purge)
      l, list       c, clean     I, info      s, search

Add y/s to the op (iy, rs, us, cs...) or -y to skip prompts.
Use `--` before a package named like an op: ryclop i -- info
Options: -h, --help   -V, --version",
    already: "«{name}» {version} is already installed: {desc}",
    available: "«{name}» {version} — {desc}",
    version: "version: {version}",
    install_confirm: "Install? [y/N]: ",
    remove_confirm: "Remove? [y/N]: ",
    purge_confirm: "Purge (package + config + orphans)? [y/N]: ",
    update_confirm: "Update repositories and system? [y/N]: ",
    clean_confirm: "Clean cache and orphans? [y/N]: ",
    select: "Pick [1-{max}] separated by spaces, or n to cancel: ",
    cancelled: "Cancelled.",
    not_found: "No matches for «{items}»",
    suggestions: "Suggestions for «{items}»:",
    installing: "Installing: {items}",
    removing: "Removing: {items}",
    purging: "Purging: {items}",
    updating: "Updating repositories and system...",
    cleaning: "Cleaning cache and orphans...",
    orphans: "Orphans: {items}",
    done: "Done.",
    unsupported: "{pm} does not support the «{op}» operation",
    no_manager: "no supported package manager was detected",
    no_root_tool: "no way to elevate privileges (doas, sudo, run0, su)",
    failed: "the package manager exited with code {code}",
    error: "error",
    system_pkgs: "=== SYSTEM / AUTOMATIC DEPENDENCIES ===",
    user_pkgs: "=== EXPLICITLY USER-INSTALLED ===",
    no_results: "No results for «{items}»",
    unknown_flag: "unknown option",
    missing_package: "missing package for «{op}»",
    unexpected_arg: "unexpected argument",
    invalid_package: "invalid package name",
};

impl Lang {
    pub const fn messages(self) -> &'static Messages {
        match self {
            Self::Es => &ES,
            Self::En => &EN,
        }
    }
}

/// POSIX locale precedence: `LC_ALL`, `LC_MESSAGES`, GNU `LANGUAGE`, `LANG`.
pub fn detect() -> Lang {
    for key in ["LC_ALL", "LC_MESSAGES", "LANGUAGE", "LANG"] {
        if let Some(value) = env::var_os(key) {
            let value = value.to_string_lossy();
            if value.is_empty() || value == "C" || value == "POSIX" {
                continue;
            }
            return if value.to_ascii_lowercase().starts_with("es") {
                Lang::Es
            } else {
                Lang::En
            };
        }
    }
    Lang::En
}
