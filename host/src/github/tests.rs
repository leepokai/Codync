use super::*;

#[test]
fn tokens_have_the_shape_github_uses() {
    assert!(token_ok("ghp_1234567890abcdefghijklmnopqrstuvw"));
    assert!(token_ok("github_pat_11ABCDEFG0abcdefghijklmnopqrstuvwxyz_0123456789abcdefghij"));
    for bad in ["", "short", "ghp_1234 5678", "ghp_1234.5678"] {
        assert!(!token_ok(bad), "{bad}");
    }
}

#[test]
fn the_status_never_carries_the_token() {
    let value = status(Some(&Config {
        token: "ghp_1234567890abcdefghijklmnopqrstuvw".into(),
        login: "octocat".into(),
        scopes: vec!["repo".into()],
        generation: "g".into(),
    }));
    assert_eq!(value["configured"], true);
    assert_eq!(value["login"], "octocat");
    assert!(value.get("token").is_none());
    assert_eq!(status(None)["configured"], false);
}
