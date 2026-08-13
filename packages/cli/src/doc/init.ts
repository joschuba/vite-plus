/**
 * `vp doc init <backend>` for the doc-command PoC (rfcs/doc-command.md).
 *
 * Scaffolds the backend's starter files (never overwrites) and adds its
 * dependencies through the normal Vite+ package-manager dispatch. The
 * interactive backend select from the RFC is not part of the PoC; the
 * backend ID is required.
 */

import fs from 'node:fs';
import path from 'node:path';

import { accent, errorMsg, log } from '../utils/terminal.ts';
import { DOC_BACKENDS, type DocBackendAdapter } from './backends.ts';
import { detectBackends, findNearestManifest } from './detect.ts';

const INIT_BACKENDS = DOC_BACKENDS.filter(
  (backend): backend is DocBackendAdapter & { init: NonNullable<DocBackendAdapter['init']> } =>
    backend.init !== undefined,
);

function initUsage(): string {
  return `Usage: vp doc init <${INIT_BACKENDS.map((backend) => backend.id).join('|')}>`;
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
  const backendId = argv[0];
  if (!backendId || backendId.startsWith('-')) {
    errorMsg(`\`vp doc init\` requires a backend ID\n\n${initUsage()}`);
    return 1;
  }

  const backend = INIT_BACKENDS.find((candidate) => candidate.id === backendId);
  if (!backend) {
    errorMsg(`unknown documentation backend \`${backendId}\`\n\n${initUsage()}`);
    return 1;
  }

  const cwd = process.cwd();
  const nearest = findNearestManifest(cwd);
  const declared = nearest ? detectBackends(nearest.manifest) : [];
  if (declared.some((candidate) => candidate.id === backend.id)) {
    log(`${accent(backend.displayName)} is already set up (\`${backend.marker}\` is declared).`);
    return 0;
  }
  if (declared.length > 0) {
    log(
      `Note: this project already declares ${declared
        .map((candidate) => `\`${candidate.marker}\``)
        .join(', ')}. ` + 'Select a backend with `--backend` after init.',
    );
  }

  for (const file of backend.init.starterFiles) {
    const target = path.join(cwd, file.path);
    if (fs.existsSync(target)) {
      log(`Kept existing ${accent(file.path)}.`);
      continue;
    }
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, file.content);
    log(`Created ${accent(file.path)}.`);
  }

  log(`Installing ${backend.init.dependencies.map((dep) => accent(dep)).join(', ')}...`);
  const exitCode = await runPm(['add', '-D', ...backend.init.dependencies]);
  if (exitCode !== 0) {
    errorMsg(`failed to install ${backend.init.dependencies.join(', ')}`);
    return exitCode;
  }

  log(`${accent(backend.displayName)} is ready. Run \`vp doc\` to start the dev server.`);
  return 0;
}
