# Shellder's shell integration for bash, the rcfile of the interactive bash a
# Console tab starts (`bash --rcfile`, see setup.sh). First what a login shell
# would have read, then the hooks that mark the prompt, the command and its
# end (OSC 133) and report the directory (OSC 7) to the Console.
# The files are of no further use (a shell on this machine keeps them):
# gone first, since a profile may end in an `exec zsh` and never come back.
[ -n "${SHELLDER_SI_KEEP-}" ] || rm -rf -- "${SHELLDER_SI_DIR:?}"
unset SHELLDER_SI_DIR SHELLDER_SI_KEEP
if [ -z "${SHELLDER_SHELL_INTEGRATION_DONE-}" ]; then
  SHELLDER_SHELL_INTEGRATION_DONE=1
  # Not a login shell as bash counts it (no `-l` with `--rcfile`), so the
  # login files are read here as bash would: the system's profile, then
  # the first of the account's own.
  [ -r /etc/profile ] && . /etc/profile
  for __shellder_f in "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile"; do
    if [ -r "$__shellder_f" ]; then . "$__shellder_f"; break; fi
  done
  unset __shellder_f

  # The directory as a file:// URL: the bytes that are not plain, escaped.
  __shellder_urlencode() {
    local LC_ALL=C s=$1 out='' i c
    for (( i = 0; i < ${#s}; i++ )); do
      c=${s:i:1}
      case $c in
        [a-zA-Z0-9/._~-]) out+=$c ;;
        *) printf -v c '%%%02X' "'$c"; out+=$c ;;
      esac
    done
    printf '%s' "$out"
  }
  # Before the prompt, first of PROMPT_COMMAND so that $? is the command's:
  # the end of the command before (a D the Console drops when no command
  # ran), the directory, the prompt's start.
  __shellder_precmd() {
    local st=$?
    printf '\033]133;D;%s\a\033]7;file://%s%s\a\033]133;A\a' "$st" "${HOSTNAME-}" "$(__shellder_urlencode "$PWD")"
  }
  # Last of PROMPT_COMMAND, after anything that sets PS1 anew: the prompt's
  # end, where the command starts.
  __shellder_mark() {
    case $PS1 in *'\[\033]133;B\a\]'*) ;; *) PS1="$PS1"'\[\033]133;B\a\]' ;; esac
  }
  if [[ "$(declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
    PROMPT_COMMAND=(__shellder_precmd "${PROMPT_COMMAND[@]}" __shellder_mark)
  else
    __shellder_pc=${PROMPT_COMMAND-}
    while [[ $__shellder_pc == *[[:space:]\;] ]]; do __shellder_pc=${__shellder_pc%?}; done
    PROMPT_COMMAND="__shellder_precmd${__shellder_pc:+;$__shellder_pc};__shellder_mark"
    unset __shellder_pc
  fi
  # The command's start, where its output begins: PS0 is written once the
  # command is read, before it runs (bash 4.4 and later; an older bash
  # gets no C, so no end and no exit status either).
  if [ "${BASH_VERSINFO[0]}" -gt 4 ] || { [ "${BASH_VERSINFO[0]}" -eq 4 ] && [ "${BASH_VERSINFO[1]}" -ge 4 ]; }; then
    PS0='\033]133;C\a'"${PS0-}"
  fi
fi
