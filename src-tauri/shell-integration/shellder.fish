# Shellder's shell integration for fish, a vendor_conf.d file on an
# XDG_DATA_DIRS entry of its own that a Console tab starts fish with (see
# setup.sh): the prompt, the command and its end are marked (OSC 133) and
# the directory reported (OSC 7) to the Console.
if status is-interactive; and not set -q SHELLDER_SHELL_INTEGRATION_DONE
    set -g SHELLDER_SHELL_INTEGRATION_DONE 1
    function __shellder_precmd --on-event fish_prompt
        set -l st $status
        printf '\e]133;D;%s\a\e]7;file://%s%s\a\e]133;A\a' $st (hostname 2>/dev/null) (string escape --style=url -- $PWD | string replace -a '%2F' '/')
    end
    function __shellder_preexec --on-event fish_preexec
        printf '\e]133;C\a'
    end
    # The prompt's end: the prompt function, wrapped at the first prompt,
    # when config.fish has set it.
    function __shellder_init --on-event fish_prompt
        functions -e __shellder_init
        if functions -q fish_prompt
            functions -c fish_prompt __shellder_orig_prompt
        else
            function __shellder_orig_prompt; printf '> '; end
        end
        function fish_prompt
            __shellder_orig_prompt
            printf '\e]133;B\a'
        end
    end
end
# The files are of no further use (a shell on this machine keeps them),
# and XDG_DATA_DIRS is as it was.
if set -q SHELLDER_SI_DIR
    if set -q SHELLDER_SI_XDG_UNSET
        set -e XDG_DATA_DIRS
    else
        # One colon-joined string to fish (not a path variable): split to take ours out.
        set -gx XDG_DATA_DIRS (string split : -- $XDG_DATA_DIRS | string match -v -- $SHELLDER_SI_DIR | string join :)
    end
    if not set -q SHELLDER_SI_KEEP
        rm -rf -- $SHELLDER_SI_DIR
    end
    set -e SHELLDER_SI_DIR SHELLDER_SI_KEEP SHELLDER_SI_XDG_UNSET
end
