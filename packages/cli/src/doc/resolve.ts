/**
 * Backend resolution for `vp doc` (rfcs/doc-command.md).
 *
 * The Rust side owns command parsing and sends a request JSON through the
 * NAPI resolver callback. This module selects the backend, validates the
 * installed package, and returns the translated execution as JSON. Thrown
 * error messages are the user-facing diagnostics.
 */

import path from 'node:path';

import semver from 'semver';

import { vite } from '../resolve-vite.ts';
import { DEFAULT_ENVS } from '../utils/constants.ts';
import { DOC_BACKENDS, type DocBackendAdapter, type DocBackendId } from './backends.ts';
import { detectBackends, findInstalledPackage, findNearestManifest } from './detect.ts';

interface ResolveDocRequest {
  action: 'dev' | 'build' | 'preview';
  backend?: string;
  args: string[];
}

interface ResolvedDocCommand {
  backend: DocBackendId;
  binPath: string;
  args: string[];
  envs: Record<string, string>;
}

function markerList(): string {
  return DOC_BACKENDS.map(
    (backend) => `  ${backend.marker}${backend.markerHint ? ` (${backend.markerHint})` : ''}`,
  ).join('\n');
}

function selectBackend(request: ResolveDocRequest, cwd: string): DocBackendAdapter {
  if (request.backend) {
    const backend = DOC_BACKENDS.find((candidate) => candidate.id === request.backend);
    if (!backend) {
      throw new Error(
        `unknown documentation backend \`${request.backend}\`\n\n` +
          `Supported backends: ${DOC_BACKENDS.map((candidate) => candidate.id).join(', ')}`,
      );
    }
    return backend;
  }

  const nearest = findNearestManifest(cwd);
  const detected = nearest ? detectBackends(nearest.manifest) : [];
  if (detected.length === 0) {
    throw new Error(
      'no documentation backend is configured\n\n' +
        'Run `vp doc init vitepress` to set up VitePress (recommended), or\n' +
        '`vp doc init ox-content`.\n\n' +
        `Or add one of these project dependencies yourself:\n${markerList()}\n\n` +
        'In a workspace, run `vp -C <dir> doc` from the documentation package.',
    );
  }
  if (detected.length > 1) {
    throw new Error(
      `multiple documentation backends are declared: ${detected
        .map((backend) => backend.id)
        .join(', ')}\n\n` +
        'Pass `--backend` or run the command from the documentation package.',
    );
  }
  return detected[0];
}

/**
 * NAPI resolver callback for `vp doc`: request JSON in, resolved command JSON
 * out. The `(err, value)` signature matches the ThreadsafeFunction contract.
 */
export async function resolveDoc(err: null | Error, requestJson: string): Promise<string> {
  if (err) {
    throw err;
  }

  const request: ResolveDocRequest = JSON.parse(requestJson);
  const cwd = process.cwd();
  const backend = selectBackend(request, cwd);

  const marker = findInstalledPackage(backend.marker, cwd);
  if (!marker) {
    throw new Error(
      `\`vp doc\` selects \`${backend.id}\`, but package \`${backend.marker}\` is not installed`,
    );
  }

  if (backend.versionRange) {
    const version = marker.packageJson.version;
    if (
      !version ||
      !semver.satisfies(version, backend.versionRange, { includePrerelease: true })
    ) {
      throw new Error(
        `\`vp doc\` supports ${backend.displayName}, but found ${backend.marker}@${version ?? 'unknown'}\n\n` +
          `Install a ${backend.displayName} release (\`${backend.versionRange}\`).`,
      );
    }
  }

  let resolved: ResolvedDocCommand;
  if (backend.target.kind === 'builtin-vite') {
    // The Vite-plugin backend participates in the normal Vite pipeline, so it
    // reuses the same resolver as the top-level dev/build/preview commands.
    const { binPath, envs } = await vite();
    resolved = { backend: backend.id, binPath, args: [request.action, ...request.args], envs };
  } else {
    const executable =
      backend.target.packageName === backend.marker
        ? marker
        : findInstalledPackage(backend.target.packageName, cwd);
    if (!executable) {
      throw new Error(
        `backend \`${backend.id}\` executes \`${backend.target.packageName}\`, but that package is not installed`,
      );
    }
    const bin = executable.packageJson.bin;
    const binRelative = typeof bin === 'string' ? bin : bin?.[backend.target.binName];
    if (!binRelative) {
      throw new Error(
        `package \`${backend.target.packageName}\` does not declare a \`${backend.target.binName}\` bin`,
      );
    }
    resolved = {
      backend: backend.id,
      binPath: path.join(executable.root, binRelative),
      args: [request.action, ...request.args],
      envs: { ...DEFAULT_ENVS },
    };
  }

  return JSON.stringify(resolved);
}
