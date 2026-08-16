# doc_info_report

`vp doc info` reports the executable as the tool; the version gate stays on the marker.

## `vp doc info`

```
Provider:  starlight (Starlight)
Source:    dependency marker `@astrojs/starlight` in package.json
Tool:      astro@7.2.2 (package-bin)
Commands:  dev, build, preview
```

## `vp doc info --json`

```
{
  "schemaVersion": 1,
  "status": "ready",
  "provider": "starlight",
  "displayName": "Starlight",
  "source": {
    "kind": "dependency-marker"
  },
  "marker": {
    "package": "@astrojs/starlight",
    "version": "0.41.7"
  },
  "execution": {
    "kind": "package-bin",
    "package": "astro",
    "version": "7.2.2",
    "bin": "astro"
  },
  "compatibility": {
    "subject": "@astrojs/starlight",
    "supportedRange": ">=0.41.0",
    "supported": true
  },
  "commands": [
    "dev",
    "build",
    "preview"
  ]
}
```
