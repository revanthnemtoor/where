where() {
    export WHERE_SHELL="bash"
    export WHERE_ALIASES="$(alias)"
    export WHERE_FUNCTIONS="$(declare -F | awk '{print $3}')"
    export WHERE_BUILTINS="$(compgen -b)"
    command where "$@"
}
