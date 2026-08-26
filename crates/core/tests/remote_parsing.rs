use agentland_core::repo::parse_remote;

#[test]
fn parses_every_url_form_a_developer_actually_uses() {
    let cases = [
        (
            "https://github.com/web3dev1337/agent-workspace.git",
            "github.com",
            "web3dev1337",
            "agent-workspace",
            "github",
        ),
        (
            "git@github.com:owner/repo.git",
            "github.com",
            "owner",
            "repo",
            "github",
        ),
        (
            "ssh://git@gitlab.com/group/subgroup/project.git",
            "gitlab.com",
            "group/subgroup",
            "project",
            "gitlab",
        ),
        (
            "https://gitlab.example.com/team/service",
            "gitlab.example.com",
            "team",
            "service",
            "gitlab",
        ),
        (
            "git@bitbucket.org:team/repo.git",
            "bitbucket.org",
            "team",
            "repo",
            "bitbucket",
        ),
        (
            "https://git.self-hosted.dev/infra/tooling.git",
            "git.self-hosted.dev",
            "infra",
            "tooling",
            "git",
        ),
    ];

    for (url, host, owner, repo, provider) in cases {
        let remote = parse_remote("origin", url);
        assert_eq!(remote.host.as_deref(), Some(host), "host for {url}");
        assert_eq!(remote.owner.as_deref(), Some(owner), "owner for {url}");
        assert_eq!(remote.repo.as_deref(), Some(repo), "repo for {url}");
        assert_eq!(remote.provider, provider, "provider for {url}");
    }
}

#[test]
fn treats_filesystem_remotes_as_local() {
    for url in ["/srv/git/project.git", "../sibling-repo", "file:///srv/git/x"] {
        let remote = parse_remote("origin", url);
        assert_eq!(remote.provider, "local", "provider for {url}");
    }
}
