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
  "provider": "starlight",
  "displayName": "Starlight",
  "source": {
    "kind": "dependency-marker",
    "marker": "@astrojs/starlight"
  },
  "target": "package-bin",
  "tool": {
    "package": "astro",
    "version": "7.2.2",
    "supportedRange": ">=0.41.0",
    "versionSupported": true
  },
  "commands": [
    "dev",
    "build",
    "preview"
  ]
}
```
