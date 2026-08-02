where() {
    export WHERE_SHELL="zsh"
    export WHERE_ALIASES="$(alias)"
    export WHERE_FUNCTIONS="$(print -l ${(k)functions})"
    export WHERE_BUILTINS="$(print -l ${(k)builtins})"
    command where "$@"
}
