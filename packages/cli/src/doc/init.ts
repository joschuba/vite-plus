/**
 * `vp doc init <provider>` for the doc-command PoC (rfcs/doc-command.md).
 *
 * Scaffolds the provider's starter files (never overwrites) and adds its
 * dependencies through the normal Vite+ package-manager dispatch. The
 * interactive provider select from the RFC is not part of the PoC; the
 * provider ID is required.
 */

import fs from 'node:fs';
import path from 'node:path';

import { accent, errorMsg, log } from '../utils/terminal.ts';
import { DOC_PROVIDERS, type DocProviderDefinition } from './providers.ts';
import { detectProviders, findNearestManifest } from './detect.ts';

const INIT_PROVIDERS = DOC_PROVIDERS.filter(
  (provider): provider is DocProviderDefinition & { init: NonNullable<DocProviderDefinition['init']> } =>
    provider.init !== undefined,
);

function initUsage(): string {
  return `Usage: vp doc init <${INIT_PROVIDERS.map((provider) => provider.id).join('|')}>`;
}

/**
 * Run `vp doc init`. `argv` holds the arguments after `init`; `runPm`
 * executes a package-manager command through the Vite+ CLI core and returns
 * its exit code.
 */
export async function runDocInit(
  argv: string[],
  runPm: (args: string[]) => Promise<number>,
): Promise<number> {
  const providerId = argv[0];
  if (!providerId || providerId.startsWith('-')) {
    errorMsg(`\`vp doc init\` requires a provider ID\n\n${initUsage()}`);
    return 1;
  }

  const provider = INIT_PROVIDERS.find((candidate) => candidate.id === providerId);
  if (!provider) {
    errorMsg(`unknown documentation provider \`${providerId}\`\n\n${initUsage()}`);
    return 1;
  }

  const cwd = process.cwd();
  const nearest = findNearestManifest(cwd);
  const declared = nearest ? detectProviders(nearest.manifest) : [];
  if (declared.some((candidate) => candidate.id === provider.id)) {
    log(`${accent(provider.displayName)} is already set up (\`${provider.marker}\` is declared).`);
    return 0;
  }
  if (declared.length > 0) {
    log(
      `Note: this project already declares ${declared
        .map((candidate) => `\`${candidate.marker}\``)
        .join(', ')}. ` + 'Select a provider with `--provider` after init.',
    );
  }

  for (const file of provider.init.starterFiles) {
    const target = path.join(cwd, file.path);
    if (fs.existsSync(target)) {
      log(`Kept existing ${accent(file.path)}.`);
      continue;
    }
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, file.content);
    log(`Created ${accent(file.path)}.`);
  }

  log(`Installing ${provider.init.dependencies.map((dep) => accent(dep)).join(', ')}...`);
  const exitCode = await runPm(['add', '-D', ...provider.init.dependencies]);
  if (exitCode !== 0) {
    errorMsg(`failed to install ${provider.init.dependencies.join(', ')}`);
    return exitCode;
  }

  log(`${accent(provider.displayName)} is ready. Run \`vp doc\` to start the dev server.`);
  return 0;
}
