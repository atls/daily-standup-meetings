use thiserror::Error;

#[derive(Debug, Error)]
pub enum MemberError {
    #[error("get_team_members returned an empty response")]
    EmptyTeamMembersResponse,

    #[error("No team members were found")]
    TeamMembersWereNotFound,

    #[error("Team members response requested another page without an end cursor")]
    TeamMembersCursorNotFound,

    #[error("get_assignable_users returned an empty response")]
    EmptyAssignableUsersResponse,

    #[error("No assignable users were found")]
    AssignableUsersWereNotFound,

    #[error("Assignable users response requested another page without an end cursor")]
    AssignableUsersCursorNotFound,
}
