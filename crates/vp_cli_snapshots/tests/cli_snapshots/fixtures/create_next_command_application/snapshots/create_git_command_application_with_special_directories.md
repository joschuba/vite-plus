# create_git_command_application_with_special_directories

## `vp create vite:application --no-interactive --git --directory 'examples with spaces/my-app'`

quote a nested target directory that contains spaces

```
◇ Scaffolded examples with spaces/my-app with Vite application
• Node <version>  pnpm <version>
→ Git (optional): git -C "examples with spaces/my-app" add -A && git -C "examples with spaces/my-app" commit -m "chore: initial commit"
→ Next: cd "examples with spaces/my-app" && vp run
```

## `vpt stat-file 'examples with spaces/my-app/.git' --assert dir`

Git repository initialized in the target directory with spaces

```
examples with spaces/my-app/.git: dir
```

## `vp create vite:application --no-interactive --git --directory 'examples;tools/my-app'`

quote a nested target directory that contains a shell metacharacter

```
◇ Scaffolded examples;tools/my-app with Vite application
• Node <version>  pnpm <version>
→ Git (optional): git -C "examples;tools/my-app" add -A && git -C "examples;tools/my-app" commit -m "chore: initial commit"
→ Next: cd "examples;tools/my-app" && vp run
```

## `vpt stat-file 'examples;tools/my-app/.git' --assert dir`

Git repository initialized in the target directory with a shell metacharacter

```
examples;tools/my-app/.git: dir
```
