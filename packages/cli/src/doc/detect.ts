/**
 * Provider detection for `vp doc` (rfcs/doc-command.md).
 *
 * Detection only reads declared dependencies from the nearest package
 * manifest. It never selects a provider because a transitive package happens
 * to resolve from node_modules.
 */

import fs from 'node:fs';
import path from 'node:path';

import { DOC_PROVIDERS, type DocProviderDefinition } from './providers.ts';

const DEPENDENCY_FIELDS = [
  'dependencies',
  'devDependencies',
  'optionalDependencies',
  'peerDependencies',
] as const;

export interface PackageManifest {
  name?: string;
  version?: string;
  bin?: string | Record<string, string>;
  [field: string]: unknown;
}

/** Read the nearest package.json, walking up from `startDir`. */
export function findNearestManifest(
  startDir: string,
): { dir: string; manifest: PackageManifest } | undefined {
  let dir = path.resolve(startDir);
  while (true) {
    const manifestPath = path.join(dir, 'package.json');
    if (fs.existsSync(manifestPath)) {
      try {
        return { dir, manifest: JSON.parse(fs.readFileSync(manifestPath, 'utf-8')) };
      } catch {
        // A malformed manifest cannot declare a provider; keep walking up.
      }
    }
    const parent = path.dirname(dir);
    if (parent === dir) {
      return undefined;
    }
    dir = parent;
  }
}

/** Providers whose marker appears in the manifest's declared dependency fields. */
export function detectProviders(manifest: PackageManifest): DocProviderDefinition[] {
  return DOC_PROVIDERS.filter((provider) =>
    DEPENDENCY_FIELDS.some((field) => {
      const deps = manifest[field];
      return typeof deps === 'object' && deps !== null && provider.marker in deps;
    }),
  );
}

/**
 * Locate an installed package by walking `node_modules` directories up from
 * `startDir`. This follows the installed layout directly instead of the
 * package's export map, so `package.json` never needs to be exported.
 */
export function findInstalledPackage(
  name: string,
  startDir: string,
): { root: string; packageJson: PackageManifest } | undefined {
  let dir = path.resolve(startDir);
  while (true) {
    const root = path.join(dir, 'node_modules', ...name.split('/'));
    const manifestPath = path.join(root, 'package.json');
    if (fs.existsSync(manifestPath)) {
      try {
        return { root, packageJson: JSON.parse(fs.readFileSync(manifestPath, 'utf-8')) };
      } catch {
        // An unreadable installed manifest counts as not installed.
      }
    }
    const parent = path.dirname(dir);
    if (parent === dir) {
      return undefined;
    }
    dir = parent;
  }
}
