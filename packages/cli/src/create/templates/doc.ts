/**
 * `vite:doc` scaffold for the doc-command PoC (rfcs/doc-command.md).
 *
 * Writes a minimal VitePress documentation site wired for `vp doc`, either
 * standalone or as a workspace package through the normal package-creation
 * flow. The files are generated in code, so no template payload ships in
 * the published package. Never writes into a non-empty directory.
 */

import fs from 'node:fs';
import path from 'node:path';

import * as prompts from '@voidzero-dev/vite-plus-prompts';

import type { WorkspaceInfo } from '../../types/index.ts';
import type { ExecutionWithProjectDir } from '../command.ts';
import type { BuiltinTemplateInfo } from './types.ts';

function docPackageJson(packageName: string): string {
  return `${JSON.stringify(
    {
      name: packageName,
      private: true,
      type: 'module',
      scripts: {
        'docs:dev': 'vp doc',
        'docs:build': 'vp doc build',
        'docs:preview': 'vp doc preview',
      },
      devDependencies: {
        // `^2.0.0-0` selects VitePress 2 releases and their prereleases.
        vitepress: '^2.0.0-0',
      },
    },
    null,
    2,
  )}\n`;
}

const DOC_VITEPRESS_CONFIG = `import { defineConfig } from 'vitepress';

export default defineConfig({
  title: 'Docs',
  description: 'Documentation site',
});
`;

const DOC_INDEX_MD = `# Hello VitePress

Start the dev server with \`vp doc\`.
`;

export async function executeDocScaffold(
  workspaceInfo: WorkspaceInfo,
  templateInfo: BuiltinTemplateInfo,
  options?: { silent?: boolean },
): Promise<ExecutionWithProjectDir> {
  const destDir = path.join(workspaceInfo.rootDir, templateInfo.targetDir);
  if (fs.existsSync(destDir) && fs.readdirSync(destDir).length > 0) {
    if (!options?.silent) {
      prompts.log.error(`target directory is not empty: ${templateInfo.targetDir}`);
    }
    return { exitCode: 1 };
  }

  fs.mkdirSync(path.join(destDir, '.vitepress'), { recursive: true });
  fs.writeFileSync(path.join(destDir, 'package.json'), docPackageJson(templateInfo.packageName));
  fs.writeFileSync(path.join(destDir, '.vitepress', 'config.ts'), DOC_VITEPRESS_CONFIG);
  fs.writeFileSync(path.join(destDir, 'index.md'), DOC_INDEX_MD);

  return { exitCode: 0, projectDir: templateInfo.targetDir };
}
