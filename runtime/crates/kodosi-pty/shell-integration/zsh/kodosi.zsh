builtin autoload -Uz add-zsh-hook

_kodosi_precmd() {
    builtin local code=$?
    if (( ${+_kodosi_running} )); then
        builtin print -n "\e]133;D;${code}\a"
        builtin unset _kodosi_running
    fi
    if [[ "${precmd_functions[-1]}" != _kodosi_precmd ]]; then
        # A later hook can still replace PS1, so this prompt gets its mark here.
        add-zsh-hook -d precmd _kodosi_precmd
        add-zsh-hook precmd _kodosi_precmd
        builtin print -n "\e]133;A;redraw=1;cl=line\a"
    elif [[ "$PS1" != *$'\e]133;A'* ]]; then
        # zsh counts a zero width space as a column, and then it redraws a full line too high.
        builtin local space=$'\xe2\x80\x8b' counted=$'%{\xe2\x80\x8b%}'
        PS1=$'%{\e]133;A;redraw=1;cl=line\a%}'"${PS1//$space/$counted}"$'%{\e]133;B\a%}'
    fi
}

_kodosi_preexec() {
    builtin print -n "\e]133;C\a"
    builtin typeset -g _kodosi_running=1
}

# The hooks start at the first prompt, so the marks wrap the prompt of the user's startup files.
_kodosi_start() {
    add-zsh-hook -d precmd _kodosi_start
    add-zsh-hook precmd _kodosi_precmd
    add-zsh-hook preexec _kodosi_preexec
}

# A precmd hook of those files can still set the first prompt, and periodic hooks run after them.
_kodosi_first_prompt() {
    add-zsh-hook -d periodic _kodosi_first_prompt
    [[ "$PERIOD" != 1 ]] || builtin unset PERIOD
    _kodosi_precmd
}

add-zsh-hook precmd _kodosi_start
add-zsh-hook periodic _kodosi_first_prompt
PERIOD=${PERIOD:-1}
