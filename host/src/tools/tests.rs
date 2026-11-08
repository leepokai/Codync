//! The shared runner against a fake engine: this test binary itself, started in a server role.
//! The server starts a grandchild of its own, so a stop that only killed the first PID would
//! leave the grandchild's port answering.

use super::runner::{Launch, Runner, State};
use super::{Engine, health, log::Log};
use anyhow::Result;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

const ROLE: &str = "CODYNC_FAKE_ENGINE";
const PORT: &str = "CODYNC_FAKE_PORT";

fn fake_launch(port: u16, role: &str) -> Result<Launch> {
    Ok(Launch {
        program: std::env::current_exe()?,
        args: ["tools::tests::fake_engine_role", "--exact", "--nocapture", "--test-threads=1"].map(Into::into).to_vec(),
        cwd: std::env::temp_dir(),
        env: vec![(ROLE.into(), role.into()), (PORT.into(), port.to_string())],
    })
}

fn engine(id: &'static str, port: u16, launch: fn(u16) -> Result<Launch>) -> &'static Engine {
    Box::leak(Box::new(Engine {
        id,
        label: "Fake",
        default_port: port,
        health_path: "/health",
        ready_timeout: Duration::from_secs(30),
        launch,
    }))
}

fn runner(engine: &'static Engine) -> Arc<Runner> {
    let logs = std::env::temp_dir().join("codync-tools-test");
    Arc::new(Runner::new(engine, Log::open(&logs, engine.id), tokio::sync::broadcast::channel(16).0))
}

fn child_port_file(parent_port: u16) -> PathBuf {
    std::env::temp_dir().join(format!("codync-fake-engine-{parent_port}.child"))
}

fn serve(listener: &TcpListener) -> ! {
    loop {
        if let Ok((mut stream, _)) = listener.accept() {
            // Read the request first: closing with unread data resets the connection on Windows and the
            // client never sees the reply.
            let mut request = [0_u8; 1024];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok");
        }
    }
}

/// Not a test: the entry point of the fake engine processes. Without the role variable it passes at once.
#[test]
fn fake_engine_role() {
    let Ok(role) = std::env::var(ROLE) else { return };
    let port: u16 = std::env::var(PORT).expect("the fake engine needs its port").parse().expect("a port number");
    match role.as_str() {
        "server" => {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).expect("binding the fake engine's port");
            let exe = std::env::current_exe().expect("the test executable");
            // The grandchild outlives nothing: the runner's job ends it with its parent, which is what the test checks.
            #[allow(clippy::zombie_processes)]
            let _grandchild = std::process::Command::new(exe)
                .args(["tools::tests::fake_engine_role", "--exact", "--nocapture"])
                .env(ROLE, "child")
                .env(PORT, port.to_string())
                .spawn()
                .expect("starting the grandchild");
            serve(&listener);
        }
        "child" => {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("binding the grandchild");
            let own = listener.local_addr().expect("its address").port();
            std::fs::write(child_port_file(port), own.to_string()).expect("writing the grandchild's port");
            serve(&listener);
        }
        "exit" => std::process::exit(3),
        other => panic!("unknown fake engine role {other}"),
    }
}

fn answers(port: u16) -> bool {
    TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), Duration::from_millis(300)).is_ok()
}

async fn gone(port: u16) -> bool {
    for _ in 0..40 {
        if !answers(port) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

#[tokio::test]
async fn starts_answers_and_stops_without_orphans() {
    let engine = engine("fake-a", 38191, |port| fake_launch(port, "server"));
    let runner = runner(engine);
    assert_eq!(runner.state(), State::Stopped);
    // A port file from an earlier run would name a grandchild that is long gone.
    let _ = std::fs::remove_file(child_port_file(38191));

    runner.start(engine.launch).await.expect("the fake engine starts");
    assert_eq!(runner.state(), State::Running);
    assert_eq!(runner.port(), 38191);
    assert!(health::probe("http://127.0.0.1:38191/health").await);

    let grandchild: u16 = loop {
        if let Ok(text) = std::fs::read_to_string(child_port_file(38191))
            && let Ok(port) = text.trim().parse()
        {
            break port;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    assert!(answers(grandchild), "the grandchild runs while the engine does");

    runner.stop().await;
    assert_eq!(runner.state(), State::Stopped);
    assert!(gone(38191).await, "the engine's port is released");
    assert!(gone(grandchild).await, "the grandchild dies with the engine");
}

#[tokio::test]
async fn takes_another_port_when_the_default_is_taken() {
    let taken = TcpListener::bind((Ipv4Addr::LOCALHOST, 38192)).expect("occupying the default port");
    let engine = engine("fake-b", 38192, |port| fake_launch(port, "server"));
    let runner = runner(engine);

    runner.start(engine.launch).await.expect("the fake engine starts elsewhere");
    assert_ne!(runner.port(), 38192);
    assert!(runner.log().tail().contains("38192 is taken"));

    runner.stop().await;
    drop(taken);
}

#[tokio::test]
async fn an_engine_that_dies_before_ready_is_an_error() {
    let engine = engine("fake-c", 38193, |port| fake_launch(port, "exit"));
    let runner = runner(engine);

    let err = runner.start(engine.launch).await.expect_err("the engine exits at once");
    assert!(format!("{err:#}").contains("stopped before it was ready"), "{err:#}");
    assert!(matches!(runner.state(), State::Error(_)));
    assert!(!runner.active());
}

/// The real thing: the unpacked portable started by the same `launch` the app uses.
/// Run with `CODYNC_COMFY_DIR=<build/native/comfy> CODYNC_HOME=<empty folder> cargo test real_comfyui -- --ignored`.
#[tokio::test]
#[ignore = "needs the unpacked ComfyUI portable (about 4.5 GB): see the doc comment"]
async fn real_comfyui_starts_validates_and_stops() {
    use super::{COMFY, comfy, comfy_api::Api, comfy_job};

    let runner = runner(&COMFY);
    runner.start(COMFY.launch).await.expect("ComfyUI starts");
    assert_eq!(runner.state(), State::Running);
    let port = runner.port();
    assert!(health::probe(&format!("http://127.0.0.1:{port}/system_stats")).await);

    // The data folders and the model links exist, and ComfyUI is using them.
    let data = comfy::data_dir();
    assert!(data.join("extra_model_paths.yaml").is_file() && data.join("models/checkpoints").is_dir());
    let info = Api::new(port).object_info().await.expect("object_info answers");
    assert!(info.get("KSampler").is_some() && info.get("SaveImage").is_some());

    // The repo's example names a checkpoint nobody has installed yet: the message says where to put it.
    let example: serde_json::Value = serde_json::from_str(include_str!("comfy_example.json")).expect("example JSON");
    let message = comfy_job::validate(&example, &info).expect_err("no checkpoint is installed").to_string();
    assert!(message.contains("v1-5-pruned-emaonly.safetensors") && message.contains("is not installed"), "{message}");

    // An unknown node is refused with a better message than ComfyUI's own.
    let unknown = serde_json::json!({"1": {"class_type": "NoSuchNode", "inputs": {}}});
    assert!(comfy_job::validate(&unknown, &info).is_err());

    // A real job that needs no model: queue it, follow it in /history, read its image from disk and from /view.
    let api = Api::new(port);
    let graph = serde_json::json!({
        "1": {"class_type": "EmptyImage", "inputs": {"width": 64, "height": 64, "batch_size": 1, "color": 16_711_680}},
        "2": {"class_type": "SaveImage", "inputs": {"images": ["1", 0], "filename_prefix": "codync-live"}},
    });
    comfy_job::validate(&graph, &info).expect("the model-free graph is valid");
    let id = api.submit(&graph, "codync").await.expect("ComfyUI queues the job");
    let record = loop {
        let record = api.history(&id).await.expect("history answers");
        if !record.is_null() {
            break record;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    assert_eq!(record["status"]["status_str"], "success", "{record}");
    let files = comfy_job::output_files(&record);
    assert_eq!(files.len(), 1, "{record}");
    let on_disk = std::fs::read(&files[0]).expect("the image is in the output folder");
    assert!(on_disk.starts_with(&[0x89, b'P', b'N', b'G']), "the saved file is a PNG");
    assert!(files[0].starts_with(comfy::output_dir()));
    let name = files[0].file_name().and_then(|n| n.to_str()).expect("a file name");
    assert_eq!(api.view(name, "", "output").await.expect("/view serves it"), on_disk);

    // An input picture goes up and comes back under the name a LoadImage node uses.
    let stored = api.upload_image("codync-input.png", on_disk.clone()).await.expect("upload");
    assert_eq!(api.view(&stored, "", "input").await.expect("the input is served"), on_disk);

    // A graph ComfyUI itself refuses comes back with the node that is wrong; the queue stays empty and idle.
    let broken = serde_json::json!({"2": {"class_type": "SaveImage", "inputs": {"filename_prefix": "x"}}});
    let refusal = api.submit(&broken, "codync").await.expect_err("a missing input is refused").to_string();
    assert!(refusal.contains("node 2 (SaveImage)"), "{refusal}");
    let queue = api.queue().await.expect("queue answers");
    assert!(
        queue["queue_running"].as_array().is_some_and(Vec::is_empty)
            && queue["queue_pending"].as_array().is_some_and(Vec::is_empty)
    );
    api.interrupt().await.expect("interrupting an idle ComfyUI is harmless");

    runner.stop().await;
    assert_eq!(runner.state(), State::Stopped);
    assert!(gone(port).await, "ComfyUI's port is released and Python is gone");
}

/// The host's side of a job with the real ComfyUI: queue, follow to `done`, one at a time, cancel mid-run.
/// Same environment as the test above.
#[tokio::test]
#[ignore = "needs the unpacked ComfyUI portable (about 4.5 GB): see the test above"]
async fn real_comfyui_jobs_run_one_at_a_time_and_cancel() {
    use crate::hub::Hub;
    use serde_json::{Value, json};

    let dir = std::env::temp_dir().join(format!("codync-jobs-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("a temp folder");
    let store = crate::store::Store::open(std::path::Path::new(":memory:")).expect("a store");
    let identity = crate::remote::identity::Identity::load_or_create(&dir).expect("an identity");
    let hub = Hub::new(store, "jobs-test".into(), identity, "t".into(), 0);

    hub.tools.start(&hub, "comfy").expect("switching ComfyUI on");
    for _ in 0..600 {
        if hub.tools.comfy_running() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(hub.tools.comfy_running(), "ComfyUI came up");
    let port = hub.tools.comfy_port();
    let jobs = hub.tools.comfy_jobs().clone();
    let phase = |jobs: &super::comfy_job::Jobs| jobs.status()["phase"].as_str().unwrap_or("none").to_owned();

    // A quick job runs to done and names its image; the bot does not exist, so only the chat attachment is skipped.
    let quick = json!({
        "1": {"class_type": "EmptyImage", "inputs": {"width": 32, "height": 32, "batch_size": 1, "color": 255}},
        "2": {"class_type": "SaveImage", "inputs": {"images": ["1", 0], "filename_prefix": "codync-job"}},
    });
    jobs.submit(&hub, "nobody", &quick).await.expect("the quick job is accepted");
    for _ in 0..120 {
        if phase(&jobs) == "done" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert_eq!(phase(&jobs), "done", "{}", jobs.status());
    assert_eq!(jobs.status()["files"].as_array().map(Vec::len), Some(1));
    assert!(jobs.first_file().is_some_and(|f| f.is_file()));

    // A slow job: a big picture blurred over and over on the processor. A second one is refused while it is active.
    let mut slow: serde_json::Map<String, Value> = serde_json::Map::new();
    slow.insert(
        "1".into(),
        json!({"class_type": "EmptyImage", "inputs": {"width": 4096, "height": 4096, "batch_size": 1, "color": 65280}}),
    );
    for step in 2..=9 {
        slow.insert(
            step.to_string(),
            json!({"class_type": "ImageBlur", "inputs": {"image": [(step - 1).to_string(), 0], "blur_radius": 31, "sigma": 5.0}}),
        );
    }
    slow.insert(
        "10".into(),
        json!({"class_type": "SaveImage", "inputs": {"images": ["9", 0], "filename_prefix": "codync-slow"}}),
    );
    let slow = Value::Object(slow);
    jobs.submit(&hub, "nobody", &slow).await.expect("the slow job is accepted");
    let second = jobs.submit(&hub, "nobody", &quick).await.expect_err("one job at a time").to_string();
    assert!(second.contains("one job at a time"), "{second}");

    for _ in 0..120 {
        if phase(&jobs) == "running" {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    assert_eq!(phase(&jobs), "running", "{}", jobs.status());
    jobs.cancel(&hub).await.expect("the running job is interrupted");
    assert_eq!(phase(&jobs), "cancelled");

    // ComfyUI really stops it: its queue empties, and no image of the slow job is announced as done.
    let api = super::comfy_api::Api::new(port);
    let mut idle = false;
    for _ in 0..240 {
        let queue = api.queue().await.expect("queue answers");
        if queue["queue_running"].as_array().is_some_and(Vec::is_empty) {
            idle = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    assert!(idle, "ComfyUI stopped the interrupted job");
    assert_eq!(phase(&jobs), "cancelled", "a late finish does not overwrite the cancellation");
    assert!(jobs.cancel(&hub).await.is_err(), "there is nothing left to cancel");

    hub.tools.stop(&hub.store, "comfy").await.expect("switching ComfyUI off");
    assert!(!hub.tools.comfy_running());
    assert!(gone(port).await, "ComfyUI is gone");
    let _ = std::fs::remove_dir_all(dir);
}
