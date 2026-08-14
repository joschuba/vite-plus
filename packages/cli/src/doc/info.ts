/**
 * `vp doc info [--json]` for the doc-command PoC (rfcs/doc-command.md).
 *
 * Reports the resolved provider, the selection source, the installed tool
 * version, and the supported commands. Never starts the tool and never
 * writes files. `--json` emits the same report for tooling and coding
 * agents. The PoC has no `doc.provider` config, so the only selection
 * source is the dependency marker.
 */

import semver from 'semver';

import { accent, errorMsg, log, muted } from '../utils/terminal.ts';
import { detectProviders, findInstalledPackage, findNearestManifest } from './detect.ts';
import type { DocProviderDefinition } from './providers.ts';

const LIFECYCLE_COMMANDS = ['dev', 'build', 'preview'] as const;

interface DocInfoReport {
  provider: string | null;
  displayName?: string;
  source?: { kind: 'dependency-marker'; marker: string };
  target?: 'package-bin' | 'builtin-vite';
  tool?: {
    package: string;
    version: string | null;
    supportedRange?: string;
    versionSupported: boolean;
  };
  commands?: string[];
  candidates?: string[];
}

function buildReport(provider: DocProviderDefinition, cwd: string): DocInfoReport {
  const installed = findInstalledPackage(provider.marker, cwd);
  const version = installed?.packageJson.version ?? null;
  const versionSupported =
    version !== null &&
    (!provider.versionRange ||
      semver.satisfies(version, provider.versionRange, { includePrerelease: true }));

  return {
    provider: provider.id,
    displayName: provider.displayName,
    source: { kind: 'dependency-marker', marker: provider.marker },
    target: provider.target.kind,
    tool: {
      package: provider.marker,
      version,
      supportedRange: provider.versionRange,
      versionSupported,
    },
    commands: [...LIFECYCLE_COMMANDS],
  };
}

function printHumanReport(report: DocInfoReport): void {
  log(`Provider:  ${accent(report.provider ?? '')} (${report.displayName})`);
  log(`Source:    dependency marker \`${report.source?.marker}\` in package.json`);
  const version = report.tool?.version ?? 'not installed';
  log(`Tool:      ${report.tool?.package}@${version} ${muted(`(${report.target})`)}`);
  if (report.tool && !report.tool.versionSupported) {
    log(
      `Warning:   installed version is unsupported` +
        (report.tool.supportedRange ? ` (requires \`${report.tool.supportedRange}\`)` : ''),
    );
  }
  log(`Commands:  ${report.commands?.join(', ')}`);
}

/** Run `vp doc info`. `argv` holds the arguments after `info`. */
export async function runDocInfo(argv: string[]): Promise<number> {
  const json = argv.includes('--json');
  const unknown = argv.find((arg) => arg !== '--json');
  if (unknown) {
    errorMsg(`unexpected argument \`${unknown}\`\n\nUsage: vp doc info [--json]`);
    return 1;
  }

  const cwd = process.cwd();
  const nearest = findNearestManifest(cwd);
  const detected = nearest ? detectProviders(nearest.manifest) : [];

  if (detected.length === 1) {
    const report = buildReport(detected[0], cwd);
    if (json) {
      process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    } else {
      printHumanReport(report);
    }
    return 0;
  }

  const report: DocInfoReport = {
    provider: null,
    candidates: detected.map((provider) => provider.id),
  };
  if (json) {
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } else if (detected.length === 0) {
    log('No documentation provider is configured. Run `vp doc init <provider>` to set one up.');
  } else {
    log(
      `Multiple documentation providers are declared: ${detected
        .map((provider) => provider.id)
        .join(', ')}. Pass \`--provider\` to lifecycle commands.`,
    );
  }
  return 1;
}
