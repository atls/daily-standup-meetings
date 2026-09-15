import { readFile } from 'node:fs/promises';
import { dirname } from 'node:path';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { GitHubClient } from './github.js';
import { formatTitle } from './config.js';
import { loadConfig } from './config.js';
import { runDsm } from './dsm.js';
const actionRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const main = async () => {
    const config = loadConfig();
    const template = await readFile(config.templatePath ?? resolve(actionRoot, 'templates', 'dsm.md'), 'utf8');
    const client = new GitHubClient(config.githubToken);
    await runDsm(client, config.owner, config.repo, config.teamSlugs, config.issueType, formatTitle(config.timezone), template);
};
main().catch((error) => {
    process.stderr.write(`${error instanceof Error ? error.message : String(error)}\n`);
    process.exitCode = 1;
});
