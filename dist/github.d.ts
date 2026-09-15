export type GitHubUser = Readonly<{
    login: string;
    nodeId: string;
}>;
export type OpenIssue = Readonly<{
    number: number;
    title: string;
}>;
export declare class GitHubClient {
    #private;
    constructor(token: string);
    teamMembers(owner: string, teamSlug: string): Promise<Array<GitHubUser>>;
    assignableUsers(owner: string, repo: string): Promise<Array<GitHubUser>>;
    issueType(owner: string, repo: string, expected: string): Promise<string>;
    latestOpenIssue(owner: string, repo: string, issueType: string): Promise<OpenIssue | undefined>;
    createIssue(owner: string, repo: string, title: string, body: string, issueType: string, assignees: Array<string>): Promise<OpenIssue>;
    closeIssue(owner: string, repo: string, number: number): Promise<void>;
    private getAll;
    private getAllPage;
    private getJson;
    private responseJson;
    private request;
    private nextPage;
    private verifyCreatedIssue;
    private endpoint;
    private errorMessage;
}
