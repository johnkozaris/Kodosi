builtin autoload -Uz add-zsh-hook

_kodosi_precmd() {
    builtin local code=$?
    if (( ${+_kodosi_running} )); then
        builtin print -n "\e]133;D;${code}\a"
        builtin unset _kodosi_running
    fi
    if [[ "$PS1" != *$'\e]133;A'* ]]; then
        PS1=$'%{\e]133;A;cl=line\a%}'"$PS1"$'%{\e]133;B\a%}'
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
    _kodosi_precmd
}

add-zsh-hook precmd _kodosi_start
