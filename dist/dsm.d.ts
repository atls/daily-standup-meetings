import type { GitHubClient } from './github.js';
import type { GitHubUser } from './github.js';
export declare const buildBody: (template: string, members: Array<GitHubUser>) => string;
export declare const selectAssignees: (members: Array<GitHubUser>, assignable: Array<GitHubUser>) => Array<string>;
export declare const runDsm: (client: GitHubClient, owner: string, repo: string, teamSlugs: Array<string>, expectedIssueType: string, title: string, template: string) => Promise<void>;
