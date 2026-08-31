# create_rejects_unsafe_target_directories

## `vp create vite:application --no-interactive --git --directory 'examples with spaces/my-app'`

reject whitespace in a nested target directory

**Exit code:** 1

```

Target directory contains unsupported character: " "
The --directory option is invalid
```

## `vp create vite:application --no-interactive --git --directory examples;touch-pwned/my-app`

reject shell metacharacters in a nested target directory

**Exit code:** 1

```

Target directory contains unsupported character: ";"
The --directory option is invalid
```
