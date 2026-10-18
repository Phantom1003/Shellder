# Shellder's shell integration: writes the files for bash, zsh and fish into
# $SHELLDER_SI_DIR, then starts the account's shell ($SHELL) so that it reads
# its own files and ours: bash with our rcfile, zsh with a ZDOTDIR of ours
# whose .zshenv puts the account's back, fish with a vendor_conf.d of ours
# on XDG_DATA_DIRS; any other shell as it is. On a host this script and the
# files are in a directory made for the tab, which the integration deletes
# once read; on this machine they are kept in the state directory
# (SHELLDER_SI_KEEP). The three files follow, each between its markers
# (the app puts them in; see core/shellint.rs).
d=${SHELLDER_SI_DIR:?}
export SHELLDER_SI_DIR SHELLDER_SHELL_INTEGRATION=1
mkdir -p "$d/bash" "$d/zsh" "$d/fish/vendor_conf.d" || exec "${SHELL:-/bin/sh}" -l
# Each file whole or not at all: written beside, then moved into place,
# so a shell reading it (two tabs opened together, on this machine) never
# sees half of it.
put() { cat > "$1.$$" && mv -f "$1.$$" "$1"; }
put "$d/bash/shellder.bash" <<'SHELLDER_EOF'
@BASH@
SHELLDER_EOF
put "$d/zsh/.zshenv" <<'SHELLDER_EOF'
@ZSH@
SHELLDER_EOF
put "$d/fish/vendor_conf.d/shellder.fish" <<'SHELLDER_EOF'
@FISH@
SHELLDER_EOF
sh=${SHELL:-/bin/sh}
case ${sh##*/} in
  bash) exec "$sh" --rcfile "$d/bash/shellder.bash" ;;
  zsh)
    if [ -n "${ZDOTDIR-}" ]; then SHELLDER_ZDOTDIR=$ZDOTDIR; export SHELLDER_ZDOTDIR; fi
    ZDOTDIR=$d/zsh exec "$sh" -l ;;
  fish)
    if [ -z "${XDG_DATA_DIRS-}" ]; then SHELLDER_SI_XDG_UNSET=1; export SHELLDER_SI_XDG_UNSET; XDG_DATA_DIRS="$d:/usr/local/share:/usr/share"; else XDG_DATA_DIRS="$d:$XDG_DATA_DIRS"; fi
    XDG_DATA_DIRS=$XDG_DATA_DIRS exec "$sh" -l ;;
  *) [ -n "${SHELLDER_SI_KEEP-}" ] || rm -rf -- "$d"; unset SHELLDER_SI_DIR SHELLDER_SI_KEEP SHELLDER_SHELL_INTEGRATION; exec "$sh" -l ;;
esac
