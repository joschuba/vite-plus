/**
 * Provider resolution for `vp doc` (rfcs/doc-command.md).
 *
 * The Rust side owns command parsing and sends a request JSON through the
 * NAPI resolver callback. This module selects the provider, validates the
 * installed package, and returns the translated execution as JSON. Thrown
 * error messages are the user-facing diagnostics.
 */

import path from 'node:path';

import semver from 'semver';

import { vite } from '../resolve-vite.ts';
import { DEFAULT_ENVS } from '../utils/constants.ts';
import { DOC_PROVIDERS, type DocProviderDefinition, type DocProviderId } from './providers.ts';
import { detectProviders, findInstalledPackage, findNearestManifest } from './detect.ts';

interface ResolveDocRequest {
  action: 'dev' | 'build' | 'preview';
  provider?: string;
  args: string[];
}

interface ResolvedDocCommand {
  provider: DocProviderId;
  binPath: string;
  args: string[];
  envs: Record<string, string>;
}

function markerList(): string {
  return DOC_PROVIDERS.map(
    (provider) => `  ${provider.marker}${provider.markerHint ? ` (${provider.markerHint})` : ''}`,
  ).join('\n');
}

function selectProvider(request: ResolveDocRequest, cwd: string): DocProviderDefinition {
  if (request.provider) {
    const provider = DOC_PROVIDERS.find((candidate) => candidate.id === request.provider);
    if (!provider) {
      throw new Error(
        `unknown documentation provider \`${request.provider}\`\n\n` +
          `Supported providers: ${DOC_PROVIDERS.map((candidate) => candidate.id).join(', ')}`,
      );
    }
    return provider;
  }

  const nearest = findNearestManifest(cwd);
  const detected = nearest ? detectProviders(nearest.manifest) : [];
  if (detected.length === 0) {
    throw new Error(
      'no documentation provider is configured\n\n' +
        'Run `vp doc init vitepress` to set up VitePress (recommended), or\n' +
        '`vp doc init ox-content`.\n\n' +
        `Or add one of these project dependencies yourself:\n${markerList()}\n\n` +
        'In a workspace, run `vp -C <dir> doc` from the documentation package.',
    );
  }
  if (detected.length > 1) {
    throw new Error(
      `multiple documentation providers are declared: ${detected
        .map((provider) => provider.id)
        .join(', ')}\n\n` +
        'Pass `--provider` or run the command from the documentation package.',
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
  const provider = selectProvider(request, cwd);

  const marker = findInstalledPackage(provider.marker, cwd);
  if (!marker) {
    throw new Error(
      `\`vp doc\` selects \`${provider.id}\`, but package \`${provider.marker}\` is not installed`,
    );
  }

  if (provider.versionRange) {
    const version = marker.packageJson.version;
    if (
      !version ||
      !semver.satisfies(version, provider.versionRange, { includePrerelease: true })
    ) {
      throw new Error(
        `\`vp doc\` supports ${provider.displayName}, but found ${provider.marker}@${version ?? 'unknown'}\n\n` +
          `Install a ${provider.displayName} release (\`${provider.versionRange}\`).`,
      );
    }
  }

  let resolved: ResolvedDocCommand;
  if (provider.target.kind === 'builtin-vite') {
    // The Vite-plugin provider participates in the normal Vite pipeline, so it
    // reuses the same resolver as the top-level dev/build/preview commands.
    const { binPath, envs } = await vite();
    resolved = { provider: provider.id, binPath, args: [request.action, ...request.args], envs };
  } else {
    const executable =
      provider.target.packageName === provider.marker
        ? marker
        : findInstalledPackage(provider.target.packageName, cwd);
    if (!executable) {
      throw new Error(
        `provider \`${provider.id}\` executes \`${provider.target.packageName}\`, but that package is not installed`,
      );
    }
    const bin = executable.packageJson.bin;
    const binRelative = typeof bin === 'string' ? bin : bin?.[provider.target.binName];
    if (!binRelative) {
      throw new Error(
        `package \`${provider.target.packageName}\` does not declare a \`${provider.target.binName}\` bin`,
      );
    }
    resolved = {
      provider: provider.id,
      binPath: path.join(executable.root, binRelative),
      args: [request.action, ...request.args],
      envs: { ...DEFAULT_ENVS },
    };
  }

  return JSON.stringify(resolved);
}
