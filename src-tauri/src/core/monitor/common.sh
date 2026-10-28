# What the reading and the tree scripts start with. `script.rs` says how
# a file of these becomes the one line that `sh -c` gets.
#
# `S name` starts a section of the output: a line of its own, the marker
# and the name. The marker carries a nonce of the run (`@MARK@` is
# replaced by it), so that a line of the host's own (a directory name
# with a newline in it) cannot start a section.
#
# Numbers come out the C way, with a point: BSD `ps` and `top` write
# `12,5` under a German locale that ssh passed on (SendEnv LANG LC_*),
# and GNU `sort -n` stops at the point. The words too (net-tools'
# `netstat` translates its own). Not LC_ALL=C, under which tmux would
# mangle a UTF-8 session name: an LC_ALL of the host's is kept for the
# characters alone.

#@ common
S(){ printf "\n@MARK@%s\n" "$1"; };
[ -n "$LC_ALL" ] && { LC_CTYPE=$LC_ALL; export LC_CTYPE; unset LC_ALL; };
LC_NUMERIC=C; LC_MESSAGES=C; export LC_NUMERIC LC_MESSAGES;
