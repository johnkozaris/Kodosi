if [[ -n "${KODOSI_ZSH_ZDOTDIR+X}" ]]; then
    builtin export ZDOTDIR="$KODOSI_ZSH_ZDOTDIR"
    builtin unset KODOSI_ZSH_ZDOTDIR
else
    builtin unset ZDOTDIR
fi
{
    builtin typeset _kodosi_file="${ZDOTDIR-$HOME}/.zshenv"
    [[ ! -r "$_kodosi_file" ]] || builtin source -- "$_kodosi_file"
} always {
    [[ ! -o interactive ]] || builtin source -- "${${(%):-%x}:A:h}/kodosi.zsh"
    builtin unset _kodosi_file
}
