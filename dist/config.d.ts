export type ActionConfig = Readonly<{
    githubToken: string;
    owner: string;
    repo: string;
    teamSlugs: Array<string>;
    issueType: string;
    templatePath?: string;
    timezone: string;
}>;
type Environment = Readonly<Record<string, string | undefined>>;
export declare const loadConfig: (environment?: Environment) => ActionConfig;
export declare const formatTitle: (timezone: string, now?: Date) => string;
export {};
