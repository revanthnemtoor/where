function where
    set -lx WHERE_SHELL "fish"
    set -lx WHERE_ALIASES (alias)
    set -lx WHERE_FUNCTIONS (functions -n)
    set -lx WHERE_BUILTINS (builtin -n)
    set -lx WHERE_ABBRS (abbr --show)
    command where $argv
end
