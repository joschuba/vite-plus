# create_git_command_application_with_spaces

## `vp create vite:application --no-interactive --git --directory 'examples with spaces/my-app'`

standalone create: quote a nested target directory that contains spaces

```
◇ Scaffolded examples with spaces/my-app with Vite application
• Node <version>  pnpm <version>
→ Git (optional): git -C 'examples with spaces/my-app' add -A && git -C 'examples with spaces/my-app' commit -m "chore: initial commit"
→ Next: cd 'examples with spaces/my-app' && vp run
```

## `vpt stat-file 'examples with spaces/my-app/.git' --assert dir`

Git repository initialized in the quoted target directory

```
examples with spaces/my-app/.git: dir
```
