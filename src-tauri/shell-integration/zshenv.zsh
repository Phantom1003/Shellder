# Shellder's shell integration for zsh: the .zshenv of a ZDOTDIR of its own
# that a Console tab starts zsh with (see setup.sh). It puts ZDOTDIR back
# first, so that zsh goes on with the account's own files, reads the
# account's .zshenv itself, and in an interactive shell hooks the prompt
# once the account's .zshrc has run: the prompt, the command and its end
# are marked (OSC 133) and the directory reported (OSC 7) to the Console.
if [[ -n ${SHELLDER_ZDOTDIR-} ]]; then ZDOTDIR=$SHELLDER_ZDOTDIR; else unset ZDOTDIR; fi
unset SHELLDER_ZDOTDIR
[[ -r ${ZDOTDIR:-$HOME}/.zshenv ]] && source "${ZDOTDIR:-$HOME}/.zshenv"
if [[ -o interactive && -z ${SHELLDER_SHELL_INTEGRATION_DONE-} ]]; then
  SHELLDER_SHELL_INTEGRATION_DONE=1
  __shellder_urlencode() {
    emulate -L zsh
    setopt no_multibyte
    local LC_ALL=C s=$1 out='' c i
    for (( i = 1; i <= ${#s}; i++ )); do
      c=${s[i]}
      case $c in
        [a-zA-Z0-9/._~-]) out+=$c ;;
        *) printf -v c '%%%02X' "'$c"; out+=$c ;;
      esac
    done
    print -rn -- "$out"
  }
  # First of the precmds, so that $? is the command's.
  __shellder_precmd() {
    local st=$?
    printf '\033]133;D;%s\a\033]7;file://%s%s\a\033]133;A\a' "$st" "${HOST-}" "$(__shellder_urlencode "$PWD")"
  }
  __shellder_preexec() { printf '\033]133;C\a'; }
  # Last of them, after a theme's own precmd has set the prompt.
  __shellder_mark() {
    [[ $PS1 == *$'%{\033]133;B\a%}'* ]] || PS1="${PS1}"$'%{\033]133;B\a%}'
  }
  # Hooked at the first prompt, when .zshrc is done: ours go round what it set.
  __shellder_init() {
    precmd_functions=(__shellder_precmd ${precmd_functions:#__shellder_(init|precmd|mark)} __shellder_mark)
    preexec_functions=(${preexec_functions:#__shellder_preexec} __shellder_preexec)
    printf '\033]7;file://%s%s\a\033]133;A\a' "${HOST-}" "$(__shellder_urlencode "$PWD")"
    __shellder_mark
  }
  precmd_functions+=(__shellder_init)
fi
[[ -n ${SHELLDER_SI_KEEP-} ]] || rm -rf -- "${SHELLDER_SI_DIR:?}"
unset SHELLDER_SI_DIR SHELLDER_SI_KEEP
