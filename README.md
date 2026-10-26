# ryclop (`ryp`)

Repositorio: <https://github.com/SrDicov/ryclop>

Envoltorio universal, mínimo y rápido para el gestor de paquetes del sistema.
Escrito en Rust, **cero dependencias externas**: solo `std` y cuatro llamadas
POSIX (`geteuid`, `newlocale`, `freelocale`, `signal`).

Estado: **v0.0.1** — funcional en xbps, pacman, apt, apk, dnf/dnf5; dpkg y rpm
como respaldo de bajo nivel (solo consulta/lista/eliminar).

## Características

- **Cero dependencias**: sin `clap`, `serde`, `tokio` ni frameworks.
- **Gestores como datos**: una tabla `Pm`/`Cmd`; añadir uno es una entrada.
- **Sin `PATH` y sin shell**: rutas absolutas; el único shell es `su -c`, con
  quoting POSIX. Nunca se invoca un binario llamado `pkg`/`ryclop`.
- **Root solo cuando toca**: `doas` → `sudo` → `run0` → `su`, y nunca se cachea
  la herramienta de elevación.
- **i18n ES/EN** según el locale; la salida que se parsea va con `LC_ALL=C.UTF-8`.
- **Sin caché ni estado en disco**.
- **Ciudadano POSIX**: `ryp l | head` termina con `SIGPIPE`, sin pánico.

## Descargar

Binarios estáticos musl para x86_64, i686, aarch64, armv7 y riscv64 en
<https://github.com/SrDicov/ryclop/releases>.

## Compilar

```sh
cargo build --release
install -m755 target/release/ryclop /usr/local/bin/ryclop
ln -s /usr/local/bin/ryclop /usr/local/bin/ryp
```

El binario release ocupa ~423 KiB con `opt-level="z"`, LTO fat,
`codegen-units=1`, `panic=abort` y `strip="symbols"`. Los `.tar.gz` de la
release incluyen el binario estático musl y el symlink relativo `ryp -> ryclop`.

## Uso

```sh
ryp <paquete>...          # atajo de `ryp i`
ryp i|install <pkg>...    # instalar
ryp u|update              # refrescar repos + actualizar el sistema
ryp r|remove <pkg>...     # desinstalar (conserva configuración)
ryp U|uninstall <pkg>...  # purgar (config + huérfanos según el gestor)
ryp c|clean               # limpiar caché y huérfanos
ryp l|list                # dependencias automáticas / instalados explícitos
ryp I|info <pkg>...       # versión y descripción
ryp s|search <término>... # buscar
```

- Sufijos `y`/`s` para no preguntar: `iy`, `is`, `rs`, `us`, `cs`, `installs`,
  `updatey`, `cleans`, etc. También `-y` / `--yes`.
- `--` obliga a tratar lo siguiente como paquete: `ryp i -- info`.
- Varios paquetes por invocación: la instalación/eliminación se ejecuta **una
  sola vez** con todos los nombres.
- Idioma automático según `LC_ALL` / `LC_MESSAGES` / `LANGUAGE` / `LANG`
  (español e inglés).
- Códigos de salida: `0` correcto, `1` error de ejecución, `2` uso inválido.

### Flags

| op | alias | efecto |
|---|---|---|
| install | `i` | instala; `ryp <pkg>` es el atajo |
| update | `u` | refresca repos y actualiza el sistema |
| remove | `r` | desinstala conservando configuración |
| uninstall | `U` | purga: configuración + huérfanos según el gestor |
| clean | `c` | limpia caché y huérfanos (agresivo) |
| list | `l` | dependencias automáticas / instalados explícitos |
| info | `I` | versión y descripción |
| search | `s` | busca en los repositorios |
| — | `y`/`s` al final | `iy`, `is`, `rs`, `us`, `cs`... no pregunta |
| — | `-y`, `--yes` | no pregunta, en cualquier posición |
| — | `-h`, `--help` | resumen de operaciones, exit `0` |
| — | `-V`, `--version` | `ryclop 0.0.1`, exit `0` |

## Flujo

1. Detecta el gestor con `stat(X_OK)` sobre rutas absolutas de una tabla estática.
   Sin caché ni estado en disco: son tres `stat` en el caso típico.
2. Resuelve todo sin root: ¿instalado? ¿existe en el repo? ¿búsqueda con
   selección numérica? Nada se ejecuta hasta tener la lista final.
3. Pide confirmación solo cuando el gestor no la ofrece (`apk`, `dpkg`, `rpm`)
   o cuando hubo desambiguación numérica. Si el gestor pregunta, la decisión es
   suya y la tomas viendo la transacción real.
4. Eleva privilegios **solo para la transacción**: root directo → `doas`
   (exige `/etc/doas.conf`) → `sudo` → `run0` → `su -c`. Nunca cachea la
   herramienta elegida.
5. Ejecuta el gestor con sus rutas absolutas. En **modo rápido** añade
   `-y`/`--noconfirm`; en modo interactivo se lo deja preguntar para que
   muestre la transacción y resuelva conflictos y proveedores virtuales.
6. Limpieza automática **no destructiva** (`autoclean`/`-Sc`/`-O`): siempre en
   modo rápido, nunca pregunta, nunca borra huérfanos sin que lo pidas. Eso es
   exclusivo de `ryp clean`.

Toda salida que `ryp` parsea se ejecuta con `LC_ALL=C.UTF-8` (con fallback a
`C` si la libc no conoce el locale); la salida del gestor que ve el usuario
conserva su idioma.

## Matriz de operaciones

`-y`/`--noconfirm` se añaden solo en modo rápido (`iy`, `-y`, ...). En modo
interactivo el gestor pide su propia confirmación cuando la tiene; `apk`, `dpkg`
y `rpm` no preguntan, así que ahí pregunta `ryp`. La fila `autoclean` es siempre
silenciosa: sus banderas van fijas.

| op | xbps | pacman | apt | apk | dnf |
|---|---|---|---|---|---|
| install | `xbps-install -S` | `pacman -S --needed` | `apt-get install` | `apk add` | `dnf install` |
| remove | `xbps-remove` | `pacman -R` | `apt-get remove` | `apk del` | `dnf remove` |
| purge | `xbps-remove -R` | `pacman -Rns` | `apt-get purge --autoremove` | `apk --purge del` | `dnf remove` ¹ |
| update | `xbps-install -S -u` | `pacman -Syu` | `apt-get update` + `full-upgrade` | `apk update` + `upgrade` | `dnf upgrade --refresh` |
| autoclean | `xbps-remove -O -y` | `pacman -Sc --noconfirm` | `apt-get autoclean` | `apk cache clean` | `dnf clean packages` |
| clean | `xbps-remove -O -o` | `pacman -Scc` + huérfanos | `apt-get autoremove --purge` + `clean` | `apk cache purge` | `dnf autoremove` + `clean all` |

¹ dnf elimina los archivos de configuración con el paquete; no hay purga aparte.

`pacman -Qtdq` devuelve 1 cuando no hay huérfanos. `clean` lo trata como lista
vacía, nunca como error.

## Añadir un gestor

Una entrada en la tabla `DRIVERS` de `src/pm.rs`. Nada de `trait`, `Box<dyn>`
ni factories: cada comando es
`Cmd { bins, args, yes, stdin }` con rutas absolutas. `yes` son las banderas de
autoconfirmación nativa; `stdin` responde prompts que no tienen bandera.
`Pm::native_prompt` dice si el gestor pregunta por su cuenta: si es `false`
(`apk`, `dpkg`, `rpm`), `ryp` asume la confirmación. Los parsers viven en
`src/parse.rs` y se seleccionan por `Kind`; la búsqueda devuelve
`Meta { name, version, description }`, no nombres pelados.

## Decisiones de seguridad

- **Nunca `PATH`**: todas las rutas son absolutas; un `PATH` envenenado no puede
  secuestrar una operación con root.
- **Nunca `pkg`/`ryclop` como destino**: evita el bucle del `pkg` de FreeBSD.
  Los alias son symlinks creados por el usuario, no parte del binario.
- **Nunca un shell** salvo `su -c`, donde cada argumento se cita con comillas
  simples POSIX.
- **Sin estado persistente**: la detección no escribe archivos, así que un
  `sudo ryp` no deja cachés `root:root` en el HOME del usuario.

## Limitaciones conocidas (v0.0.1)

- Faltan zypper, emerge, el `pkg` de FreeBSD/Termux y Nix. La tabla está lista.
- `remove`/`uninstall` de dpkg/rpm y `install` de paquetes locales (`.deb`,
  `.rpm`, `.xbps`) aún no existen.
- Sin corrección de typos por distancia de edición: la búsqueda nativa hace el
  trabajo. Usa `--` para nombres que choquen con una operación.
- `updates` (inglés) actualiza sin preguntar, tal como se especificó; si te
  parece peligroso, usa `ryp u`.
- `dnf` no tiene purga separada de configuración.
- `xbps` puede necesitar una segunda pasada de `update` cuando actualiza
  `xbps` mismo; `ryp` no la encadena todavía.
- `list` separa **dependencias automáticas** de **instalados explícitamente**;
  no pretende adivinar la "base del sistema" (el kernel y `base` son explícitos).
- `ryp s` imprime la salida nativa del gestor (en `C`/inglés) para conservar
  versiones y descripciones; el menú de `ryp i` sí usa esos metadatos.
- La limpieza posterior puede pedir credenciales dos veces si el elevador no
  cachea tickets (`doas` sin `persist`); los comandos no se agrupan bajo un
  shell de root a propósito.

## Desarrollo

```sh
cargo fmt
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo doc --no-deps
```

## Licencia

GPL-3.0-only. Texto completo en [`LICENSE`](LICENSE).
