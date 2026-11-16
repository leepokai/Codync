#![allow(clippy::unwrap_used)]

use super::client;
use super::daemon::{Keeper, start};
use super::record::{Keep, OAuth, Record};
use super::store::{Kind, Secrets};
use crate::store::Store;
use axum::extract::{Request, State};
use axum::response::IntoResponse;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use zeroize::Zeroizing;

fn temp_dir() -> PathBuf {
    // Short: a unix socket path has a length limit.
    let dir = std::env::temp_dir().join(format!("ck-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A keeper serving from a temp folder on its own runtime (the host's client blocks while it waits).
pub(crate) struct TestKeeper {
    pub keeper: Arc<Keeper>,
    pub socket: PathBuf,
}

impl TestKeeper {
    /// Points `store` at this keeper, and makes it a cloud computer's.
    pub fn manage(&self, store: &Store) {
        store.kv_set("test_keeper_socket", &self.socket.to_string_lossy()).unwrap();
        crate::remote::managed::set_managed(store).unwrap();
    }
}

/// A keeper that already has its vault key.
pub(crate) fn test_keeper() -> TestKeeper {
    let keeper = locked_keeper();
    keeper.keeper.set_key(Zeroizing::new(crate::remote::crypto::random()));
    keeper
}

fn locked_keeper() -> TestKeeper {
    let dir = temp_dir();
    let socket = dir.join("k.sock");
    let (tx, rx) = std::sync::mpsc::channel();
    let path = socket.clone();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
        runtime.block_on(async move {
            let (keeper, serve) = start(&dir, &path, 0).await.unwrap();
            tx.send(keeper).unwrap();
            let _ = serve.await;
        });
    });
    TestKeeper { keeper: rx.recv().unwrap(), socket }
}

fn memory_store() -> Store {
    Store::open(std::path::Path::new(":memory:")).unwrap()
}

#[test]
fn records_open_only_in_their_own_slot_and_kind() {
    let dir = temp_dir();
    let key = [7u8; 32];
    let secrets = Secrets::open(&dir).unwrap();
    secrets.put(&key, "a", Kind::Visible, b"value").unwrap();
    assert_eq!(&*secrets.get(&key, "a").unwrap().unwrap().1, b"value");
    assert!(secrets.get(&[8u8; 32], "a").is_err(), "another key opens nothing");
    let file = dir.join("secrets.json");
    let text = std::fs::read_to_string(&file).unwrap();
    assert!(!text.contains("value"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    }
    // Moved to another slot, or relabelled proxied, the record no longer opens.
    let mut saved: Value = serde_json::from_str(&text).unwrap();
    let item = saved["items"]["a"].clone();
    saved["items"]["b"] = item.clone();
    saved["items"]["c"] = json!({"kind": "proxied", "sealed": item["sealed"]});
    std::fs::write(&file, saved.to_string()).unwrap();
    let reopened = Secrets::open(&dir).unwrap();
    assert!(reopened.get(&key, "a").is_ok());
    assert!(reopened.get(&key, "b").is_err());
    assert!(reopened.get(&key, "c").is_err());
    assert!(reopened.delete("b").unwrap());
    assert!(!reopened.delete("b").unwrap());
    assert_eq!(reopened.names(""), ["a", "c"]);
    assert_eq!(reopened.kind("c"), Some(Kind::Proxied));
}

#[test]
fn the_socket_answers_every_op_and_never_reveals_a_proxied_secret() {
    let k = test_keeper();
    let store = memory_store();
    k.manage(&store);
    assert_eq!(client::public_key(&store).unwrap(), k.keeper.identity.sign_pub_b64());
    let status = client::status(&store).unwrap().unwrap();
    assert!(status.unlocked);
    assert_eq!(status.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(status.boot, k.keeper.boot);
    assert_ne!(status.boot, test_keeper().keeper.boot, "a new boot id each start");
    let nobody = memory_store();
    nobody.kv_set("test_keeper_socket", &temp_dir().join("none.sock").to_string_lossy()).unwrap();
    assert!(client::status(&nobody).unwrap().is_none(), "no keeper running");

    client::put(&store, "logins", "[1]").unwrap();
    assert_eq!(client::reveal(&store, "logins").unwrap().as_deref(), Some("[1]"));
    assert_eq!(client::reveal(&store, "nothing").unwrap(), None);
    assert!(client::has(&store, "logins").unwrap());
    assert!(client::put(&store, "a/b", "x").is_err(), "slot names are checked");

    let record = Record::header("https://api.example.com", "x-api-key", "sk-secret".into());
    client::put_proxied(&store, "voice:openai", &record, &Keep::default()).unwrap();
    let refused = client::reveal(&store, "voice:openai").unwrap_err().to_string();
    assert!(!refused.contains("sk-secret") && refused.contains("never read"), "{refused}");
    assert_eq!(client::names(&store, "voice:").unwrap(), ["voice:openai"]);
    let bad = Record::header("http://api.example.com", "x-api-key", "k".into());
    assert!(client::put_proxied(&store, "voice:gemini", &bad, &Keep::default()).is_err(), "https only");

    let grant = client::granted(&store, "voice:openai", Some(Duration::from_mins(1))).unwrap();
    assert_eq!(grant.url, format!("http://127.0.0.1:{}/t/{}", k.keeper.port, grant.token));
    assert!(client::granted(&store, "logins", None).is_err(), "only proxied records are granted");
    assert!(k.keeper.granted(&grant.token).is_some());
    let line = json!({"op": "revoke", "token": grant.token});
    assert_eq!(raw(&k.socket, &line.to_string()), json!({"ok": true}));
    assert!(k.keeper.granted(&grant.token).is_none());

    let grant = client::granted(&store, "voice:openai", None).unwrap();
    client::delete(&store, "voice:openai").unwrap();
    assert!(!client::has(&store, "voice:openai").unwrap());
    assert!(k.keeper.granted(&grant.token).is_none(), "deleting ends its grants");

    assert_eq!(
        raw(&k.socket, "{\"op\":\"put\",\"slot\":\"x\",\"value\":\"secret\",\"extra\":1}"),
        json!({"error": "bad request"})
    );
    let long = format!("{{\"op\":\"put\",\"slot\":\"x\",\"value\":\"{}\"}}", "a".repeat(1024 * 1024));
    assert_eq!(raw(&k.socket, &long), json!({"error": "request too long"}));
}

/// One request line, the answer as JSON.
fn raw(socket: &std::path::Path, line: &str) -> Value {
    use std::io::{BufRead as _, Write as _};
    let stream = std::os::unix::net::UnixStream::connect(socket).unwrap();
    (&stream).write_all(format!("{line}\n").as_bytes()).unwrap();
    let mut answer = String::new();
    std::io::BufReader::new(&stream).read_line(&mut answer).unwrap();
    serde_json::from_str(&answer).unwrap()
}

#[test]
fn a_locked_keeper_keeps_its_secrets_shut() {
    let k = locked_keeper();
    let store = memory_store();
    k.manage(&store);
    assert!(!client::status(&store).unwrap().unwrap().unlocked);
    assert_eq!(client::put(&store, "x", "v").unwrap_err().to_string(), client::LOCKED_MESSAGE);
    assert!(!client::has(&store, "x").unwrap(), "names need no key");
}

#[test]
fn a_cloud_computers_vault_is_its_keeper_and_any_other_stays_the_same() {
    use crate::market::vault;
    let k = test_keeper();
    let store = memory_store();
    k.manage(&store);
    vault::write(&store, "connectors", "[]").unwrap();
    assert_eq!(store.kv_read("connectors").unwrap(), None, "nothing in the host's database");
    assert_eq!(vault::read(&store, "connectors").unwrap().as_deref(), Some("[]"));
    assert!(vault::saved(&store, "connectors").unwrap());
    assert!(client::has(&store, "connectors").unwrap());

    // A computer of the user's own, with a keeper there: its vault never asks it.
    let own = memory_store();
    own.kv_set("test_keeper_socket", &k.socket.to_string_lossy()).unwrap();
    vault::write(&own, "logins", "[2]").unwrap();
    assert!(own.kv_read("logins").unwrap().unwrap().starts_with("vault:v1:"));
    assert_eq!(vault::read(&own, "logins").unwrap().as_deref(), Some("[2]"));
    assert!(!client::has(&store, "logins").unwrap());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_cloud_computers_unlock_waits_for_its_keeper() {
    let k = locked_keeper();
    let dir = temp_dir();
    let store = Store::open(&dir.join("t.db")).unwrap();
    k.manage(&store);
    let hub = crate::hub::Hub::new(
        store,
        "test".into(),
        crate::remote::identity::Identity::load_or_create(&dir).unwrap(),
        "test".into(),
        0,
    );
    let keeper = k.keeper.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(600)).await;
        keeper.set_key(Zeroizing::new([1; 32]));
    });
    crate::market::vault::unlock(hub.clone()).await.unwrap();
    assert!(hub.store.secret_key.lock().unwrap().is_none(), "the host never holds a vault key");
}

// MARK: the proxy

#[derive(Default)]
struct Upstream {
    /// What each request brought: method, path, query, headers.
    seen: std::sync::Mutex<Vec<Value>>,
    token_calls: AtomicUsize,
    /// The access token the upstream accepts at `/oauth`.
    valid: std::sync::Mutex<String>,
}

async fn upstream() -> (String, Arc<Upstream>, Arc<tokio::sync::Notify>) {
    let state = Arc::new(Upstream::default());
    let release = Arc::new(tokio::sync::Notify::new());
    let app = axum::Router::new()
        .route("/token", axum::routing::post(token))
        .route(
            "/stream",
            axum::routing::get({
                let release = release.clone();
                move || async move {
                    let events = futures::stream::unfold(0, move |n| {
                        let release = release.clone();
                        async move {
                            match n {
                                0 => Some((Ok::<_, std::io::Error>("data: one\n\n"), 1)),
                                1 => {
                                    release.notified().await;
                                    Some((Ok("data: two\n\n"), 2))
                                }
                                _ => None,
                            }
                        }
                    });
                    ([("content-type", "text/event-stream")], axum::body::Body::from_stream(events))
                }
            }),
        )
        .route(
            "/redirect",
            axum::routing::get(|| async { axum::response::Redirect::temporary("https://evil.example/") }),
        )
        .fallback(echo)
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (base, state, release)
}

async fn echo(State(s): State<Arc<Upstream>>, request: Request) -> axum::response::Response {
    let headers: serde_json::Map<String, Value> =
        request.headers().iter().map(|(k, v)| (k.to_string(), json!(v.to_str().unwrap_or_default()))).collect();
    let seen = json!({"method": request.method().as_str(), "path": request.uri().path(), "query": request.uri().query(), "headers": headers});
    s.seen.lock().unwrap().push(seen.clone());
    if request.uri().path().starts_with("/oauth") {
        let wanted = format!("Bearer {}", s.valid.lock().unwrap());
        if request.headers().get("authorization").and_then(|v| v.to_str().ok()) != Some(wanted.as_str()) {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
    }
    axum::Json(seen).into_response()
}

async fn token(State(s): State<Arc<Upstream>>, body: String) -> axum::response::Response {
    let n = s.token_calls.fetch_add(1, Ordering::SeqCst) + 1;
    let form: std::collections::HashMap<String, String> =
        url::form_urlencoded::parse(body.as_bytes()).into_owned().collect();
    if form.get("grant_type").map(String::as_str) != Some("refresh_token")
        || form.get("refresh_token") != Some(&format!("refresh-{}", n - 1))
    {
        return (axum::http::StatusCode::BAD_REQUEST, axum::Json(json!({"error": "invalid_grant"}))).into_response();
    }
    *s.valid.lock().unwrap() = format!("access-{n}");
    axum::Json(
        json!({"access_token": format!("access-{n}"), "refresh_token": format!("refresh-{n}"), "expires_in": 3600}),
    )
    .into_response()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_proxy_sends_the_secret_only_to_its_upstream() {
    let (base, up, release) = upstream().await;
    let k = test_keeper();
    let mut record = Record::header(&format!("{base}/v1"), "x-api-key", "sk-real".into());
    record.strip_query = vec!["key".into()];
    k.keeper.put_record("agent", &record).unwrap();
    let grant = k.keeper.grant("agent", Some(60)).unwrap().url;
    let http = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none()).build().unwrap();

    let answer: Value = http
        .post(format!("{grant}/messages?key=codync-keeper&alt=sse"))
        .header("x-api-key", "codync-keeper")
        .header("authorization", "Bearer codync-keeper")
        .header("cookie", "a=b")
        .header("x-goog-api-key", "codync-keeper")
        .header("anthropic-version", "2023-06-01")
        .body("hello")
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(answer["method"], "POST");
    assert_eq!(answer["path"], "/v1/messages", "below the record's upstream");
    assert_eq!(answer["query"], "alt=sse", "the key parameter is dropped");
    let headers = &answer["headers"];
    assert_eq!(headers["x-api-key"], "sk-real");
    assert_eq!(headers["anthropic-version"], "2023-06-01");
    for gone in ["authorization", "cookie", "x-goog-api-key"] {
        assert!(headers.get(gone).is_none(), "{gone}");
    }
    assert_eq!(headers["host"], base.trim_start_matches("http://"));

    // Unknown, revoked and expired grants: 401, and the upstream never hears of them. One without a time limit lasts.
    let short = k.keeper.grant("agent", Some(1)).unwrap().url;
    let lasting = [k.keeper.grant("agent", None).unwrap().url, k.keeper.grant("agent", Some(0)).unwrap().url];
    for url in [format!("http://127.0.0.1:{}/t/nope/x", k.keeper.port), format!("http://127.0.0.1:{}/x", k.keeper.port)]
    {
        let refused = http.get(url).send().await.unwrap();
        assert_eq!(refused.status(), 401);
        assert!(refused.text().await.unwrap().is_empty(), "no detail");
    }
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(http.get(format!("{short}/x")).send().await.unwrap().status(), 401);
    assert_eq!(up.seen.lock().unwrap().len(), 1);
    for url in &lasting {
        assert_eq!(http.get(format!("{url}/x")).send().await.unwrap().status(), 200);
    }
    assert_eq!(up.seen.lock().unwrap().len(), 3);
    // TRACE would echo the request back, the key with it: only the methods an API takes go through.
    let trace = http.request(reqwest::Method::TRACE, format!("{}/x", lasting[0])).send().await.unwrap();
    assert_eq!(trace.status(), 405);
    assert_eq!(up.seen.lock().unwrap().len(), 3, "the upstream never hears of it");

    // An event stream flows as the upstream sends it.
    let mut record = record.clone();
    record.upstream = base.clone();
    k.keeper.put_record("stream", &record).unwrap();
    let stream = k.keeper.grant("stream", None).unwrap().url;
    // A redirect comes back as it is: following it would take the key along.
    let moved = http.get(format!("{stream}/redirect")).send().await.unwrap();
    assert_eq!(moved.status(), 307);
    assert_eq!(moved.headers()["location"], "https://evil.example/");
    let mut events = http.get(format!("{stream}/stream")).send().await.unwrap();
    let first = tokio::time::timeout(Duration::from_secs(5), events.chunk()).await.unwrap().unwrap().unwrap();
    assert_eq!(&first[..], b"data: one\n\n", "the first event arrives while the upstream still holds the second");
    release.notify_one();
    let second = tokio::time::timeout(Duration::from_secs(5), events.chunk()).await.unwrap().unwrap().unwrap();
    assert_eq!(&second[..], b"data: two\n\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn the_keeper_refreshes_a_sign_in_when_it_expires_and_once_on_a_401() {
    let (base, up, _) = upstream().await;
    let k = test_keeper();
    let oauth = |refresh: &str, expires_at| OAuth {
        token_url: format!("{base}/token"),
        client_id: "codync".into(),
        client_secret: None,
        refresh_token: Some(refresh.into()),
        expires_at,
        header: "authorization".into(),
        resource: Some(format!("{base}/oauth")),
    };
    let mut record = Record::header(&format!("{base}/oauth"), "authorization", "Bearer access-0".into());
    record.oauth = Some(oauth("refresh-0", 1));
    k.keeper.put_record("connector:a", &record).unwrap();
    let grant = k.keeper.grant("connector:a", None).unwrap().url;

    // Expired: refreshed before the request goes out.
    let answer = reqwest::get(format!("{grant}/x")).await.unwrap();
    assert_eq!(answer.status(), 200);
    assert_eq!(up.token_calls.load(Ordering::SeqCst), 1);
    let saved = k.keeper.record("connector:a").unwrap().unwrap();
    assert_eq!(saved.headers["authorization"], "Bearer access-1");
    let saved_oauth = saved.oauth.clone().unwrap();
    assert_eq!(saved_oauth.refresh_token.as_deref(), Some("refresh-1"), "the rotated refresh token is kept");
    assert!(saved_oauth.expires_at > 1);

    // Ended early (the upstream says 401 with a token that looks valid): refreshed once, then sent again.
    *up.valid.lock().unwrap() = "revoked".into();
    let answer = reqwest::get(format!("{grant}/x")).await.unwrap();
    assert_eq!(answer.status(), 200);
    assert_eq!(up.token_calls.load(Ordering::SeqCst), 2);
    assert_eq!(k.keeper.record("connector:a").unwrap().unwrap().headers["authorization"], "Bearer access-2");

    // A refresh the token endpoint refuses: the upstream's 401 comes back, after one try, and the sign-in is over,
    // which the host sees.
    let store = memory_store();
    k.manage(&store);
    assert!(client::signed_in(&store, "connector:a").unwrap());
    let mut stale = k.keeper.record("connector:a").unwrap().unwrap();
    stale.oauth = Some(oauth("refresh-wrong", 0));
    k.keeper.put_record("connector:a", &stale).unwrap();
    *up.valid.lock().unwrap() = "revoked".into();
    assert_eq!(reqwest::get(format!("{grant}/x")).await.unwrap().status(), 401);
    assert_eq!(up.token_calls.load(Ordering::SeqCst), 3);
    assert!(!client::signed_in(&store, "connector:a").unwrap());
    let ended = k.keeper.record("connector:a").unwrap().unwrap();
    assert!(ended.oauth.is_none() && !ended.headers.contains_key("authorization"), "nothing of it is sent again");

    // One without a refresh token ends at the first 401, without asking the token endpoint.
    let mut once = Record::header(&format!("{base}/oauth"), "authorization", "Bearer access-0".into());
    once.oauth = Some(OAuth { refresh_token: None, ..oauth("", 0) });
    k.keeper.put_record("connector:b", &once).unwrap();
    let grant = k.keeper.grant("connector:b", None).unwrap().url;
    assert_eq!(reqwest::get(format!("{grant}/x")).await.unwrap().status(), 401);
    assert_eq!(up.token_calls.load(Ordering::SeqCst), 3);
    assert!(!client::signed_in(&store, "connector:b").unwrap());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_keeper_asks_the_cloud_for_its_key_until_it_answers() {
    #[derive(Default)]
    struct Cloud {
        asked: AtomicUsize,
        kids: std::sync::Mutex<Vec<String>>,
    }
    async fn vault_key(State(c): State<Arc<Cloud>>, headers: axum::http::HeaderMap) -> axum::response::Response {
        let sig = headers.get("codync-sig").and_then(|v| v.to_str().ok()).unwrap_or_default();
        let kid = sig.split(',').find_map(|p| p.strip_prefix("kid=")).unwrap_or_default().to_owned();
        c.kids.lock().unwrap().push(kid);
        match c.asked.fetch_add(1, Ordering::SeqCst) {
            0 => axum::http::StatusCode::NOT_FOUND.into_response(),
            1 => axum::http::StatusCode::BAD_GATEWAY.into_response(),
            _ => axum::Json(json!({"key": crate::remote::crypto::b64(&[9u8; 32])})).into_response(),
        }
    }
    let cloud = Arc::new(Cloud::default());
    let app =
        axum::Router::new().route("/v1/keeper/vault-key", axum::routing::get(vault_key)).with_state(cloud.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });

    let k = locked_keeper();
    assert!(!k.keeper.unlocked());
    tokio::time::timeout(Duration::from_secs(10), super::unlock::run(k.keeper.clone(), base)).await.unwrap();
    assert!(k.keeper.unlocked());
    assert_eq!(*k.keeper.key().unwrap(), [9u8; 32]);
    assert_eq!(cloud.asked.load(Ordering::SeqCst), 3);
    let pubkey = k.keeper.identity.sign_pub_b64();
    assert!(cloud.kids.lock().unwrap().iter().all(|kid| *kid == pubkey), "signed by the keeper it named");
}
