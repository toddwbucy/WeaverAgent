use serde_json::json;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::Path;
use weaver_ste_driver::gate::{Capture, Transport, TurnClient};

fn capture(value: serde_json::Value) -> Capture {
    Capture {
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: b"retained diagnostic".to_vec(),
        exit_code: Some(0),
        error: None,
    }
}

#[test]
fn completed_answers_keep_the_exact_text_and_finish() {
    for finish in [None, Some("length")] {
        let mut value = json!({"kind":"answered","run":"r-1","turn":"t-2",
            "text":"  Read [[C1]].\r\n"});
        if let Some(finish) = finish {
            value["finish"] = json!(finish);
        }
        let capture = capture(value);
        let answer = capture.answer().unwrap();
        assert_eq!(answer.run, "r-1");
        assert_eq!(answer.turn, "t-2");
        assert_eq!(answer.text.as_deref(), Some("  Read [[C1]].\r\n"));
        assert_eq!(answer.truncated, finish.is_some());
        assert_eq!(capture.to_json()["stderr_bytes"][0], b'r');
    }
}

#[test]
fn only_a_completed_answer_can_have_a_missing_output_shape_fault() {
    for text in [serde_json::Value::Null, json!(7), json!("")] {
        let capture = capture(json!({"kind":"answered","run":"r","turn":"t","text":text}));
        let answer = capture.answer().unwrap();
        assert!(answer.text.is_none() || answer.text.as_deref() == Some(""));
    }
    for value in [
        json!({"kind":"stopped","run":"r","turn":"t","reason":"unload"}),
        json!({"kind":"refused","reason":"not ready"}),
        json!({"kind":"answered","text":"answer"}),
        json!({"kind":"answered","run":"r","turn":"","text":"answer"}),
        json!({"kind":"answered","run":"r","turn":"t","finish":"stop"}),
        json!(null),
    ] {
        assert!(capture(value).answer().is_err());
    }
    let mut failure = capture(json!({"kind":"answered","run":"r","turn":"t","text":"x"}));
    failure.exit_code = Some(1);
    assert!(failure.answer().is_err());
    failure.exit_code = Some(0);
    failure.error = Some("start failed".into());
    assert!(failure.answer().is_err());
    failure.error = None;
    failure.stdout = b"not JSON".to_vec();
    assert!(failure.answer().is_err());
}

#[test]
fn the_real_turn_client_sends_one_user_block_and_keeps_raw_evidence() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("ste-gate-{}-{nanos}", std::process::id()));
    let agent = "ste-fixture";
    let admin = root.join("admin");
    let coordination = root.join("coord");
    std::fs::create_dir_all(admin.join(agent)).unwrap();
    std::fs::create_dir_all(coordination.join(format!("weaver-{agent}"))).unwrap();
    std::fs::write(
        admin.join(agent).join("coordination-root"),
        coordination.to_str().unwrap(),
    )
    .unwrap();
    let socket = coordination.join(format!("weaver-{agent}/gate.sock"));
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    let request = "<<<TEXT\nRead `data` and $(literal).\nTEXT>>>";
    let response =
        b"{\"kind\":\"answered\",\"run\":\"r\",\"turn\":\"t\",\"text\":\"Read the file.\"}\n";
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let (mut peer, _) = loop {
            match listener.accept() {
                Ok(peer) => break peer,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "client did not connect"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("accept: {error}"),
            }
        };
        peer.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut line = String::new();
        BufReader::new(peer.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&line).unwrap(),
            json!({"text":request})
        );
        peer.write_all(response).unwrap();
    });
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let mut client = TurnClient::new(repo, agent, &admin).unwrap();
    let result = client.send(request);
    server.join().unwrap();
    assert_eq!(result.stdout, response);
    assert_eq!(
        result.answer().unwrap().text.as_deref(),
        Some("Read the file.")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_real_client_retains_its_preconnection_failure() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap();
    let missing = std::env::temp_dir().join(format!("ste-no-config-{}", std::process::id()));
    assert!(!missing.exists());
    let mut client = TurnClient::new(repo, "ste-fixture", &missing).unwrap();
    let result = client.send("<<<TEXT\nRead the file.\nTEXT>>>");
    assert_eq!(result.exit_code, Some(1));
    assert!(result.stdout.is_empty());
    assert!(String::from_utf8_lossy(&result.stderr).contains("cannot read"));
    assert!(result.answer().is_err());
}
