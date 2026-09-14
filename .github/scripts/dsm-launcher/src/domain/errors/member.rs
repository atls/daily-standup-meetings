use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemberError {
    #[error("get_team_members returned an empty response")]
    EmptyTeamMembersResponse,

    #[error("No team members were found")]
    TeamMembersWereNotFound,

    #[error("Team members response requested another page without an end cursor")]
    TeamMembersCursorNotFound,
}
