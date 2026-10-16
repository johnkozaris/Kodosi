# Kodosi's prompt marks (OSC 133) for bash. Kodosi starts bash in POSIX mode
# with ENV set to this file, so bash reads it before any startup file. The file
# leaves POSIX mode, reads the startup files that bash reads, and adds the marks.
[ -n "${BASH_VERSION-}" ] || return 0

if [[ -n "${KODOSI_BASH_INJECT-}" ]]; then
    builtin set +o posix
    builtin unset KODOSI_BASH_INJECT ENV
    if [[ -n "${KODOSI_BASH_ENV+X}" ]]; then
        builtin export ENV="$KODOSI_BASH_ENV"
        builtin unset KODOSI_BASH_ENV
    fi
    if [[ -n "${KODOSI_BASH_UNEXPORT_HISTFILE-}" ]]; then
        builtin export -n HISTFILE
        builtin unset KODOSI_BASH_UNEXPORT_HISTFILE
    fi
    if builtin shopt -q login_shell; then
        [[ ! -r /etc/profile ]] || builtin source /etc/profile
        for _kodosi_file in "$HOME/.bash_profile" "$HOME/.bash_login" "$HOME/.profile"; do
            if [[ -r "$_kodosi_file" ]]; then
                builtin source "$_kodosi_file"
                break
            fi
        done
        builtin unset _kodosi_file
    elif [[ -r "$HOME/.bashrc" ]]; then
        builtin source "$HOME/.bashrc"
    fi
fi

if (( BASH_VERSINFO[0] > 4 || (BASH_VERSINFO[0] == 4 && BASH_VERSINFO[1] >= 4) )); then
    _kodosi_prompt_start() {
        _kodosi_code=$?
        if [[ -n "${_kodosi_running+X}" ]]; then
            builtin printf '\e]133;D;%s\a' "$_kodosi_code"
            builtin unset _kodosi_running
        fi
        return "$_kodosi_code"
    }

    _kodosi_prompt_end() {
        if [[ "$PS1" != *'\e]133;B\a'* ]]; then
            PS1+='\[\e]133;B\a\]'
        fi
        builtin printf '\e]133;A;redraw=last;cl=line\a'
        return "$_kodosi_code"
    }

    if builtin shopt -q promptvars; then
        PS0='\e]133;C\a${_kodosi_running:=}'"${PS0-}"
    fi
    if [[ "$(builtin declare -p PROMPT_COMMAND 2>/dev/null)" == "declare -a"* ]]; then
        PROMPT_COMMAND=(_kodosi_prompt_start "${PROMPT_COMMAND[@]}" _kodosi_prompt_end)
    else
        PROMPT_COMMAND=_kodosi_prompt_start$'\n'"${PROMPT_COMMAND-}"$'\n'_kodosi_prompt_end
    fi
fi
