//! The shell integration of the Console's shells: the account's shell on a
//! host (or on this machine) marks its prompt, the command and its end
//! (OSC 133) and reports its directory (OSC 7), which is what lets the
//! Console jump between prompts, open a new tab in the same directory, name
//! the tab after it, tell when a long command is done and with what status,
//! and ask before closing a tab with a command still running. The scripts
//! are in `shell-integration/` beside `src/`: one for bash, one for zsh,
//! one for fish, and `setup.sh`, which writes the three where the shell
//! will find them and starts the shell. A host gets the whole of it in the
//! ssh command (`remote_command`): base64 in a one-line `sh -c`, since
//! nothing may be assumed on the host but a Bourne shell and `base64` or
//! `openssl`; the files land in a directory made for the tab and are
//! deleted once the shell has read them. On this machine the setup script
//! is written to the state directory and kept (`local_setup`).

use std::path::PathBuf;

use base64::Engine;

use super::config;

const SETUP: &str = include_str!("../../shell-integration/setup.sh");
const BASH: &str = include_str!("../../shell-integration/shellder.bash");
const ZSH: &str = include_str!("../../shell-integration/zshenv.zsh");
const FISH: &str = include_str!("../../shell-integration/shellder.fish");

/// The setup script with the three files in it.
pub fn setup_script() -> String {
    SETUP
        .replacen("@BASH@", BASH.trim_end(), 1)
        .replacen("@ZSH@", ZSH.trim_end(), 1)
        .replacen("@FISH@", FISH.trim_end(), 1)
}

/// A string as `printf %b` reads it, inside double quotes: every byte an
/// octal escape, so that a path goes through any login shell's parsing of
/// the ssh command untouched (no quote, no space, no `!` in it).
fn octal(s: &str) -> String {
    s.bytes().map(|b| format!("\\0{b:03o}")).collect()
}

/// The ssh command for a shell on a host, `dir` the directory it starts
/// in (home without): a `sh -c` on one line that the account's login
/// shell, whatever it is, passes on to sh: decode the setup script into
/// a directory of its own and run it, which starts the shell; without a
/// `base64` or `openssl` to decode with, the login shell as it is.
pub fn remote_command(dir: Option<&str>) -> String {
    let b64 = base64::engine::general_purpose::STANDARD.encode(setup_script());
    let cd = match dir {
        Some(d) => format!("cd \"$(printf %b \"{}\")\" 2>/dev/null; ", octal(d)),
        None => String::new(),
    };
    format!(
        "sh -c '{cd}b={b64}; d=${{TMPDIR:-/tmp}}/.shellder-si.$$; \
         if mkdir -m 700 \"$d\" 2>/dev/null && {{ printf %s \"$b\" | base64 -d > \"$d/setup\" 2>/dev/null \
         || printf %s \"$b\" | base64 -D > \"$d/setup\" 2>/dev/null \
         || printf %s \"$b\" | openssl base64 -d -A > \"$d/setup\" 2>/dev/null; }} && [ -s \"$d/setup\" ]; \
         then SHELLDER_SI_DIR=\"$d\" exec sh \"$d/setup\"; fi; rm -rf \"$d\" 2>/dev/null; exec \"${{SHELL:-sh}}\" -l'"
    )
}

/// The plain command for a shell on a host in `dir` when the integration
/// is off: what the Console sent before it had one.
pub fn plain_command(dir: &str) -> String {
    format!(
        "cd '{}' 2>/dev/null; exec \"${{SHELL:-sh}}\" -l",
        dir.replace('\'', "'\\''")
    )
}

/// The setup script written to the state directory for a shell on this
/// machine: the directory (what `SHELLDER_SI_DIR` is to be) and the
/// script's path. Written afresh every time, so a newer app's scripts
/// replace an older one's.
pub fn local_setup() -> Result<(PathBuf, PathBuf), String> {
    config::ensure_dirs();
    let dir = config::get().state_dir.join("shell-integration");
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join("setup.sh");
    // Whole or not at all: a shell may be reading it (two tabs at once).
    let tmp = dir.join(format!("setup.sh.{}", std::process::id()));
    std::fs::write(&tmp, setup_script()).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((dir, path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_carries_the_three_files() {
        let s = setup_script();
        assert!(s.contains("__shellder_precmd() {"), "bash");
        assert!(s.contains("precmd_functions+=(__shellder_init)"), "zsh");
        assert!(s.contains("--on-event fish_prompt"), "fish");
        assert!(!s.contains("@BASH@") && !s.contains("@ZSH@") && !s.contains("@FISH@"));
        // The heredoc's marker must not occur in what it quotes.
        assert_eq!(s.matches("SHELLDER_EOF").count(), 6);
    }

    #[test]
    fn remote_command_is_one_line_of_plain_characters() {
        let c = remote_command(Some("/tmp/it's here!"));
        assert!(!c.contains('\n'));
        // One pair of single quotes, the login shell's: nothing inside may
        // end it, and no `!` for csh to expand.
        assert_eq!(c.matches('\'').count(), 2);
        assert!(!c[5..c.len() - 1].contains('!'));
        assert!(c.starts_with("sh -c 'cd \"$(printf %b \"\\0057\\0164"));
        assert!(remote_command(None).starts_with("sh -c 'b="));
    }

    #[test]
    fn octal_escapes_every_byte() {
        assert_eq!(octal("a/é"), "\\0141\\0057\\0303\\0251");
    }
}
