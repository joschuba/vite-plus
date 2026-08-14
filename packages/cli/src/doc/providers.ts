/**
 * Data-only registry of documentation providers for `vp doc`.
 *
 * PoC scope (rfcs/doc-command.md): VitePress 2 as a package-bin target and
 * Ox Content as the built-in Vite target. The remaining RFC providers join by
 * adding entries here; detection and resolution stay generic.
 */

export type DocProviderId = 'vitepress' | 'ox-content';

export type DocProviderTarget =
  | { kind: 'package-bin'; packageName: string; binName: string }
  | { kind: 'builtin-vite' };

export interface DocProviderInit {
  /** Dependency specs added through the project's package manager. */
  dependencies: string[];
  /** Files written only when missing, relative to the effective root. */
  starterFiles: { path: string; content: string }[];
}

export interface DocProviderDefinition {
  id: DocProviderId;
  /** Human name used in diagnostics. */
  displayName: string;
  /** Declared dependency that selects this provider. */
  marker: string;
  /** Extra hint rendered next to the marker in the no-provider error. */
  markerHint?: string;
  /** Supported semver range for the marker package, checked before execution. */
  versionRange?: string;
  target: DocProviderTarget;
  /** One-command setup support for `vp doc init`. */
  init?: DocProviderInit;
}

export const DOC_PROVIDERS: readonly DocProviderDefinition[] = [
  {
    id: 'vitepress',
    displayName: 'VitePress 2',
    marker: 'vitepress',
    markerHint: 'major version 2',
    versionRange: '>=2.0.0-0 <3.0.0',
    target: { kind: 'package-bin', packageName: 'vitepress', binName: 'vitepress' },
    init: {
      // `next` is the VitePress 2 dist-tag while 2.0 is prerelease; a range
      // spec replaces it when 2.0 reaches `latest`.
      dependencies: ['vitepress@next'],
      starterFiles: [
        {
          path: 'index.md',
          content: '# Hello VitePress\n\nStart the dev server with `vp doc`.\n',
        },
      ],
    },
  },
  {
    id: 'ox-content',
    displayName: 'Ox Content',
    marker: '@ox-content/vite-plugin',
    target: { kind: 'builtin-vite' },
    init: {
      dependencies: ['@ox-content/vite-plugin'],
      starterFiles: [
        {
          path: 'vite.config.ts',
          content:
            "import { oxContent } from '@ox-content/vite-plugin';\n\n" +
            'export default {\n' +
            "  plugins: [oxContent({ srcDir: 'docs' })],\n" +
            '};\n',
        },
        {
          path: 'docs/index.md',
          content: '# Hello Ox Content\n\nStart the dev server with `vp doc`.\n',
        },
        {
          path: 'index.html',
          content:
            '<!doctype html>\n<html>\n  <head>\n    <meta charset="utf-8" />\n' +
            '    <title>Docs</title>\n  </head>\n  <body>\n    <div id="app"></div>\n' +
            '  </body>\n</html>\n',
        },
      ],
    },
  },
];
