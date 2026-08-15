# doc_workspace_listing

Non-interactive at a workspace root: each candidate prints as a ready-to-run command.

## `vp doc build`

**Exit code:** 1

```
[1m[31merror:[39m[0m several workspace packages declare a documentation provider

  vp -C packages/docs doc build      (starlight)
  vp -C packages/handbook doc build  (vitepress)
```
