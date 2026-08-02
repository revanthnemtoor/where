function where
    set -lx WHERE_SHELL "fish"
    set -lx WHERE_ALIASES (alias | string join \n)
    set -lx WHERE_FUNCTIONS (functions -n | string join \n)
    set -lx WHERE_BUILTINS (builtin -n | string join \n)
    set -lx WHERE_ABBRS (abbr --show | string join \n)
    command where $argv
end
