use super::*;

#[test]
fn repeated_runtime_flags_and_positional_values_keep_their_order() {
    let mut c: Connector = serde_json::from_value(json!({"id":"fixture", "name":"Fixture"})).unwrap();
    let p = json!({"registryType":"oci", "identifier":"example/fixture:1", "runtimeArguments":[
        {"type":"named", "name":"-i"},
        {"type":"named", "name":"-e", "value":"KEY=one"},
        {"type":"named", "name":"-e", "value":"KEY=two"},
        {"type":"positional", "value":"KEY=one"},
        {"type":"positional", "value":"KEY=one"}
    ]});
    configure_package(&mut c, &p, &|_| None).unwrap();
    assert_eq!(
        c.args,
        ["run", "--rm", "-i", "-e", "KEY=one", "-e", "KEY=two", "KEY=one", "KEY=one", "example/fixture:1"]
    );
}

#[test]
fn shared_input_names_keep_the_strongest_masking_and_requirement() {
    let server = json!({"packages":[{"registryType":"oci", "identifier":"example/fixture:1",
        "environmentVariables":[{"name":"token"}],
        "runtimeArguments":[{"type":"named", "name":"-e", "value":"KEY={token}",
            "variables":{"token":{"isSecret":true, "isRequired":true}}}]}]});
    let options = install_options(&server);
    assert_eq!(options[0]["inputs"].as_array().unwrap().len(), 1);
    assert_eq!(options[0]["inputs"][0]["name"], "token");
    assert_eq!(options[0]["inputs"][0]["secret"], true);
    assert_eq!(options[0]["inputs"][0]["required"], true);
}

#[tokio::test]
async fn only_declared_templates_expand_and_fixed_values_are_not_credentials() {
    let store = crate::store::Store::open(std::path::Path::new(":memory:")).unwrap();
    let server = json!({"packages":[{"registryType":"oci", "identifier":"example/fixture:1",
        "environmentVariables":[{"name":"token", "isSecret":true}],
        "runtimeArguments":[
            {"type":"named", "name":"--path", "valueHint":"path", "isRequired":true},
            {"type":"named", "name":"-e", "value":"KEY={token}:{port}",
             "variables":{"token":{"isSecret":true}, "port":{"value":"8085"}}}],
        "packageArguments":[{"type":"named", "name":"--key", "value":"{token}",
                             "variables":{"token":{"isSecret":true}}}]}]});
    let added = install_server(
        &store,
        &server,
        "test/fixture",
        "package:0",
        &json!({"token":"literal{port}", "--path":"/tmp/{token}"}),
    )
    .await
    .unwrap();
    assert_eq!(added["keys"], json!(["token"]));
    install_server(&store, &server, "test/fixture", "package:0", &json!({})).await.unwrap();
    let c = connectors(&store).unwrap().pop().unwrap();
    let launched = c.launch_args(&c.env);
    assert!(launched.contains(&"/tmp/{token}".to_owned()));
    assert!(launched.contains(&"KEY=literal{port}:8085".to_owned()));
    assert_eq!(launched.last().unwrap(), "literal{port}");
}

#[tokio::test]
async fn literal_braces_in_an_input_do_not_rebuild_a_saved_install() {
    let store = crate::store::Store::open(std::path::Path::new(":memory:")).unwrap();
    let server = json!({"packages":[{"registryType":"oci", "identifier":"example/fixture:1",
        "runtimeArguments":[{"type":"named", "name":"--path", "valueHint":"path", "isRequired":true}]}]});
    let first =
        install_server(&store, &server, "test/literal", "package:0", &json!({"--path":"/tmp/{token}"})).await.unwrap();
    let again = install_server(&store, &server, "test/literal", "package:0", &json!({})).await.unwrap();
    assert_eq!(first, again);
}

#[tokio::test]
async fn installing_again_with_new_values_rebuilds_the_connector_in_place() {
    let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("store");
    let package = |kind: &str| {
        json!({"registryType": kind, "identifier": "server", "version": "1.0.0",
               "environmentVariables": [{"name": "API_KEY", "isRequired": kind == "npm"}]})
    };
    let name = "io.example/server";
    let server = json!({"name": name, "packages": [package("npm"), package("pypi")]});
    let only = || connectors(&store).expect("read").pop().expect("one connector");
    let first = install_server(&store, &server, name, "package:0", &json!({"API_KEY": "wrong"})).await.expect("add");
    install_server(&store, &server, name, "package:0", &json!({"API_KEY": "right"})).await.expect("new key");
    // Add again with nothing typed keeps it (and its sign-in) as it is.
    install_server(&store, &server, name, "package:0", &json!({"API_KEY": ""})).await.expect("retry");
    assert_eq!(
        (only().id, only().env["API_KEY"].clone()),
        (first["id"].as_str().expect("id").to_owned(), "right".into())
    );
    install_server(&store, &server, name, "package:1", &json!({})).await.expect("another runtime");
    assert_eq!(only().command.as_deref(), Some("uvx"));
    assert_eq!(connectors(&store).expect("read").len(), 1);
}

#[test]
fn oci_references_do_not_need_a_separate_package_version() {
    for id in [
        "ghcr.io/github/github-mcp-server:2.0.2".to_owned(),
        "localhost:5000/team/server:2.0.2".to_owned(),
        "server:v2.0.2-rc.1".to_owned(),
        format!("ghcr.io/team/server@sha256:{}", "a".repeat(64)),
        format!("ghcr.io/team/server:latest@sha256:{}", "b".repeat(64)),
    ] {
        let mut package = json!({"registryType": "oci", "identifier": id});
        assert_eq!(package_reference(&package).unwrap(), id);
        // Older registry entries also supply a separate package version.
        package["version"] = json!("0.20.0");
        assert_eq!(package_reference(&package).unwrap(), id);
    }
}

#[test]
fn package_metadata_cannot_pin_a_floating_or_invalid_oci_reference() {
    for id in [
        "ghcr.io/github/github-mcp-server",
        "localhost:5000/team/server",
        "server:latest",
        "server:",
        "server:^2.0.2",
        "server:2.*",
        "server:2.0.2 invalid",
        "server:2.0.2\n",
        "server@sha256:",
        "server@sha256:not-a-digest",
        "server:2.0.2@sha256:invalid",
    ] {
        for version in [Value::Null, json!("2.0.2")] {
            let package = json!({"registryType": "oci", "identifier": id, "version": version});
            assert!(package_reference(&package).is_err(), "accepted {package}");
        }
    }
}

#[test]
fn other_registries_still_require_their_own_package_version() {
    for (kind, expected) in [("npm", "server@2.0.2"), ("pypi", "server==2.0.2"), ("nuget", "server@2.0.2")] {
        let mut package = json!({"registryType": kind, "identifier": "server", "version": "2.0.2"});
        assert_eq!(package_reference(&package).unwrap(), expected);
        for version in [Value::Null, json!(""), json!("latest"), json!("^2.0.2"), json!("~2.0.2"), json!("*")] {
            package["version"] = version;
            assert!(package_reference(&package).is_err(), "accepted {package}");
        }
    }
}

/// The Registry's GitHub connector entry, as the host receives it.
fn github_server(host_port: u16) -> Value {
    json!({"name": "io.github.github/github-mcp-server", "packages": [{
        "registryType": "oci",
        "identifier": "ghcr.io/github/github-mcp-server:2.0.3",
        "transport": {"type": "stdio"},
        "runtimeArguments": [
            {"type": "named", "name": "-p", "value": format!("127.0.0.1:{host_port}:8085")},
            {"type": "named", "name": "-e", "value": "GITHUB_OAUTH_CALLBACK_PORT=8085"},
            {"type": "named", "name": "-e", "value": "GITHUB_PERSONAL_ACCESS_TOKEN={token}",
             "variables": {"token": {"format": "string", "isSecret": true}}},
        ],
    }]})
}

#[tokio::test]
async fn a_docker_connector_keeps_every_registry_argument_and_its_credential() {
    let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("store");
    let name = "io.github.github/github-mcp-server";
    let server = github_server(8085);
    // A token left out leaves its argument out too, so the server signs in on first use.
    install_server(&store, &server, name, "package:0", &json!({})).await.expect("add");
    let only = || connectors(&store).expect("read").pop().expect("one connector");
    assert_eq!(
        only().args,
        [
            "run",
            "-i",
            "--rm",
            "-p",
            "127.0.0.1:8085:8085",
            "-e",
            "GITHUB_OAUTH_CALLBACK_PORT=8085",
            "ghcr.io/github/github-mcp-server:2.0.3"
        ]
    );
    assert!(only().env.is_empty());
    // Adding a token rebuilds it in place: both `-e` flags stay, and the token is a credential.
    install_server(&store, &server, name, "package:0", &json!({"token": "ghp_x"})).await.expect("add a token");
    let c = only();
    assert_eq!(
        c.args,
        [
            "run",
            "-i",
            "--rm",
            "-p",
            "127.0.0.1:8085:8085",
            "-e",
            "GITHUB_OAUTH_CALLBACK_PORT=8085",
            "-e",
            "GITHUB_PERSONAL_ACCESS_TOKEN={token}",
            "ghcr.io/github/github-mcp-server:2.0.3"
        ]
    );
    assert_eq!(c.env["token"], "ghp_x");
    assert_eq!(c.public()["keys"], json!(["token"]), "the token stays editable in Credentials");
}

#[tokio::test]
async fn adding_an_old_install_again_rebuilds_its_malformed_arguments() {
    let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("store");
    let name = "io.github.github/github-mcp-server";
    // What the pre-credential installer saved: the literal template, and the second `-e` lost.
    let old: Connector = serde_json::from_value(json!({
        "id": "github", "name": "GitHub", "registryName": name, "command": "docker",
        "args": ["run", "-i", "--rm", "-p", "127.0.0.1:8085:8085", "-e",
                 "GITHUB_OAUTH_CALLBACK_PORT=8085", "GITHUB_PERSONAL_ACCESS_TOKEN={token}",
                 "ghcr.io/github/github-mcp-server:2.0.3"]
    }))
    .expect("old connector");
    add_connector(&store, old).expect("save the old install");
    install_server(&store, &github_server(8085), name, "package:0", &json!({})).await.expect("add again");
    let c = connectors(&store).expect("read").pop().expect("connector");
    assert_eq!(
        c.args,
        [
            "run",
            "-i",
            "--rm",
            "-p",
            "127.0.0.1:8085:8085",
            "-e",
            "GITHUB_OAUTH_CALLBACK_PORT=8085",
            "ghcr.io/github/github-mcp-server:2.0.3"
        ],
        "the argument whose variable has no value is left out, and the command is well formed"
    );
    assert!(c.env.is_empty());
}

/// The whole install → launch path against Docker and the real image: needs Docker.
#[tokio::test]
#[ignore = "needs Docker and the GitHub MCP image"]
async fn a_registry_docker_connector_starts_and_lists_its_tools() {
    let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("store");
    // A free host port: the Registry asks for 8085, which another app on this computer may hold.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("free port");
    let server = github_server(listener.local_addr().expect("address").port());
    drop(listener);
    let name = "io.github.github/github-mcp-server";
    install_server(&store, &server, name, "package:0", &json!({"token": "ghp_e2e_dummy"})).await.expect("install");
    let id = connectors(&store).expect("read").pop().expect("connector").id;
    let ready = crate::market::verify::verify(&store, &id, 0).await.expect("the connector starts");
    assert!(ready["toolCount"].as_u64().unwrap_or(0) > 10, "the GitHub server lists its tools: {ready}");
}

#[test]
fn pasted_configs_list_their_servers() {
    let claude =
        json!({"mcpServers": {"github": {"command": "npx", "args": ["-y", "x"]}, "linear": {"url": "https://l/mcp"}}});
    assert_eq!(config_servers(&claude).len(), 2);
    let vscode = json!({"servers": {"a": {"type": "sse", "url": "https://a/sse"}}});
    assert_eq!(config_servers(&vscode)[0].0, "a");
    let one = json!({"command": "uvx", "args": ["y"]});
    assert_eq!(config_servers(&one)[0].0, "Custom");
    let bare = json!({"fs": {"command": "npx"}});
    assert_eq!(config_servers(&bare)[0].0, "fs");
}
