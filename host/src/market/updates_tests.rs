use super::*;
use crate::market::save_connectors;
use serde_json::json;
use std::collections::BTreeMap;

fn connector(command: &str, args: &[&str]) -> Connector {
    Connector {
        id: "github".into(),
        name: "GitHub".into(),
        description: String::new(),
        registry_name: Some("io.github.github/github-mcp-server".into()),
        command: Some(command.into()),
        args: args.iter().map(|arg| (*arg).into()).collect(),
        env: BTreeMap::from([("TOKEN".into(), "secret".into())]),
        url: None,
        headers: BTreeMap::new(),
        oauth: None,
    }
}

fn server(kind: &str, identifier: &str, version: Option<&str>) -> Value {
    let mut package = json!({
        "registryType": kind,
        "identifier": identifier,
        "transport": {"type": "stdio"}
    });
    if let Some(version) = version {
        package["version"] = json!(version);
    }
    json!({
        "name": "io.github.github/github-mcp-server",
        "packages": [package]
    })
}

#[test]
fn updates_the_matching_package_argument_and_keeps_other_arguments() {
    let c = connector("npx", &["-y", "@scope/github-mcp@1.2.3", "--mode", "safe"]);
    let change = replacement(&c, &server("npm", "@scope/github-mcp", Some("1.3.0")))
        .expect("matching package")
        .expect("new version");
    assert_eq!(change.index, 1);
    assert_eq!(change.reference, "@scope/github-mcp@1.3.0");
}

#[test]
fn updates_oci_images_when_package_version_is_omitted() {
    let c = connector("docker", &["run", "-i", "--rm", "ghcr.io/github/github-mcp-server:2.0.2"]);
    let change = replacement(&c, &server("oci", "ghcr.io/github/github-mcp-server:2.0.3", None))
        .expect("matching image")
        .expect("new image");
    assert_eq!(change.index, 3);
    assert_eq!(change.reference, "ghcr.io/github/github-mcp-server:2.0.3");
}

#[test]
fn refuses_registry_downgrades() {
    let c = connector("npx", &["-y", "github-mcp@2.0.2"]);
    let result = replacement(&c, &server("npm", "github-mcp", Some("2.0.1")));
    assert!(result.is_err());
}

#[test]
fn apply_latest_preserves_credentials() {
    let store = Store::open(std::path::Path::new(":memory:")).expect("store");
    let before = connector("npx", &["-y", "github-mcp@1.0.0"]);
    save_connectors(&store, std::slice::from_ref(&before)).expect("save");
    assert!(apply_latest(&store, &before, &server("npm", "github-mcp", Some("1.1.0"))).expect("update"));
    let after = connectors(&store).expect("read").pop().expect("connector");
    assert_eq!(after.args, ["-y", "github-mcp@1.1.0"]);
    assert_eq!(after.env, before.env);
}
