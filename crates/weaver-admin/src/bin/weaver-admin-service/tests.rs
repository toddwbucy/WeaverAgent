//! The service's tests, per `weaver-admin-operator-contract` section 8. **No
//! test reaches sudo**: the test-only constructor below sets the sudo path to
//! a stand-in script that records its argv and answers as each test arranges,
//! and the root program's path is a name the stand-in receives and never runs.
use super::*;
use std::io::BufRead;
use std::os::unix::fs::PermissionsExt;

const INSTANCE: &str = "0123456789abcdef";
const CLOCK_MS: u64 = 1_795_651_200_000;

/// The service's own log, kept for the test to read.
#[derive(Clone, Default)]
struct SharedLog(Arc<Mutex<Vec<u8>>>);

impl Write for SharedLog {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl SharedLog {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).into_owned()
    }
}

impl Service {
    /// **The test-only constructor**: the sudo path is the test's stand-in,
    /// the connector and the service are the test's own uid, the root's owner
    /// is that uid, the waits are short, and the ids and the clock count, so a
    /// transcript has one byte form.
    fn for_test(sudo: PathBuf, admin: PathBuf, base: PathBuf, log: SharedLog) -> Service {
        let uid = nix::unistd::getuid().as_raw();
        let next = AtomicUsize::new(0);
        Service {
            sudo,
            admin,
            base,
            root_owner: uid,
            connector_uid: uid,
            own_uid: uid,
            instance: INSTANCE.into(),
            timing: Timing {
                opening: Duration::from_millis(600),
                judge_cut: Duration::from_millis(1500),
                open_after_connect: Duration::from_secs(1),
                grant: Duration::from_millis(1500),
                log_tick: Duration::from_millis(20),
            },
            connections_bound: CONNECTIONS,
            ids: Box::new(move || Ok(format!("{:016x}", next.fetch_add(1, Ordering::SeqCst) + 1))),
            clock: Box::new(|| CLOCK_MS),
            log: Mutex::new(Box::new(log)),
            agents: Mutex::new(HashMap::new()),
            connections: AtomicUsize::new(0),
        }
    }
}

/// The stand-in for sudo. A grant read (`-n -l <admin> <verb> <agent>`)
/// answers by the files `granted` (one verb a line, exit 0), `grant-<verb>`
/// (an exit code) and `grant-sleep`, and exits 1 otherwise. A run (`-n
/// <admin> <verb> <agent>`) sources `run-<verb>` where it stands, and prints
/// a small object otherwise. Every call appends its argv to `argv`, each
/// argument followed by `|`.
const STAND_IN: &str = r#"#!/bin/sh
d='@DIR@'
for a in "$@"; do printf '%s|' "$a"; done >>"$d/argv"
echo >>"$d/argv"
if [ "$1" = -n ] && [ "$2" = -l ]; then
  if [ -e "$d/vanish" ]; then chmod 000 "$0"; fi
  if [ -e "$d/grant-sleep" ]; then sleep "$(cat "$d/grant-sleep")"; fi
  if [ -e "$d/grant-$4" ]; then echo "the grant read of $4" >&2; exit "$(cat "$d/grant-$4")"; fi
  if grep -qx -- "$4" "$d/granted" 2>/dev/null; then echo "    $3 $4 $5"; exit 0; fi
  echo "Sorry, user may not run $4 on $5." >&2
  exit 1
fi
if [ -e "$d/run-$3" ]; then . "$d/run-$3"; fi
printf '{"verb":"%s","agent":"%s"}\n' "$3" "$4"
"#;

struct Fixture {
    dir: PathBuf,
    socket: PathBuf,
    admin: PathBuf,
    log: SharedLog,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// **An executable is written by a child process** (#497): a description of
/// it held open for writing in this process could be inherited by another
/// test's fork and refuse its exec `ETXTBSY`.
fn write_executable(path: &Path, text: &str) {
    let staged = path.with_extension("staged");
    std::fs::write(&staged, text).unwrap();
    let status = Command::new("/bin/cp")
        .arg("--")
        .arg(&staged)
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::remove_file(&staged).unwrap();
}

fn fixture(tag: &str) -> Fixture {
    fixture_with(tag, |_| {})
}

fn fixture_with(tag: &str, adjust: impl FnOnce(&mut Service)) -> Fixture {
    let dir =
        std::env::temp_dir().join(format!("weaver-admin-service-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let stand_in = dir.join("stand-in");
    write_executable(
        &stand_in,
        &STAND_IN.replace("@DIR@", &dir.display().to_string()),
    );
    let base = dir.join("roots");
    let root = base.join("a1");
    let territory = dir.join("territory");
    let coordination = dir.join("coordination");
    for d in [
        &base,
        &root,
        &territory,
        &coordination.join("weaver.run").join("a1"),
    ] {
        std::fs::create_dir_all(d).unwrap();
        std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    for (key, value) in [
        ("territory", &territory),
        ("coordination-root", &coordination),
    ] {
        let path = root.join(key);
        std::fs::write(&path, format!("{}\n", value.display())).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    }
    let log_file = territory.join("admin.log");
    std::fs::write(&log_file, "").unwrap();
    std::fs::set_permissions(&log_file, std::fs::Permissions::from_mode(0o640)).unwrap();
    let admin = dir.join("bin").join("weaver-admin");
    let log = SharedLog::default();
    let mut service = Service::for_test(stand_in, admin.clone(), base, log.clone());
    adjust(&mut service);
    let socket = dir.join("service.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let service = Arc::new(service);
    std::thread::spawn(move || serve(service, listener));
    Fixture {
        dir,
        socket,
        admin,
        log,
    }
}

impl Fixture {
    fn put(&self, name: &str, text: &str) {
        std::fs::write(self.dir.join(name), text).unwrap();
    }
    fn argv(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("argv"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
    fn admin(&self) -> String {
        self.admin.display().to_string()
    }
    /// The argv line of a grant read.
    fn read_line(&self, verb: &str) -> String {
        format!("-n|-l|{}|{verb}|a1|", self.admin())
    }
    /// The argv line of a run.
    fn run_line(&self, verb: &str) -> String {
        format!("-n|{}|{verb}|a1|", self.admin())
    }
    fn log_path(&self) -> PathBuf {
        self.dir.join("territory").join("admin.log")
    }
    fn append(&self, text: &str) {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(self.log_path())
            .unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }
    fn client(&self) -> Client {
        let stream = UnixStream::connect(&self.socket).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let reader = std::io::BufReader::new(stream.try_clone().unwrap());
        Client { stream, reader }
    }
    /// A connection opened for `purpose`, its `open` read.
    fn opened(&self, purpose: &str) -> Client {
        let mut client = self.client();
        client.send(&opening(purpose, "a1"));
        assert_eq!(client.line(), open_line(), "the opening is answered open");
        client
    }
}

fn opening(purpose: &str, agent: &str) -> String {
    format!(r#"{{"weaver_admin":{{"version":1,"purpose":"{purpose}","agent":"{agent}"}}}}"#)
}

fn open_line() -> String {
    format!(r#"{{"open":{{"version":1,"instance":"{INSTANCE}"}}}}"#)
}

fn verb_ask(verb: &str) -> String {
    format!(
        r#"{{"verb":{{"verb":"{verb}","principal":{{"kind":"person","person_id":"pe-0123456789abcdef"}}}}}}"#
    )
}

const RUN_ASK: &str = r#"{"run":{}}"#;

struct Client {
    stream: UnixStream,
    reader: std::io::BufReader<UnixStream>,
}

impl Client {
    fn send(&mut self, line: &str) {
        self.stream
            .write_all(format!("{line}\n").as_bytes())
            .unwrap();
    }
    /// The next line, without its newline, or "" at the close.
    fn line(&mut self) -> String {
        let mut line = String::new();
        self.reader.read_line(&mut line).unwrap();
        line.trim_end_matches('\n').to_string()
    }
    /// True where the service closed with no further byte.
    fn closed(&mut self) -> bool {
        // A peer that closes with bytes of ours unread resets the connection
        // on Linux rather than ending it, so a reset with nothing read is a
        // close too.
        let mut rest = Vec::new();
        match self.reader.read_to_end(&mut rest) {
            Ok(_) => rest.is_empty(),
            Err(e) => e.kind() == std::io::ErrorKind::ConnectionReset && rest.is_empty(),
        }
    }
    fn value(&mut self) -> Value {
        let line = self.line();
        serde_json::from_str(&line).unwrap_or_else(|_| panic!("a line: {line:?}"))
    }
}

fn word(value: &Value) -> &str {
    value["refused"]["word"].as_str().unwrap_or("")
}

/// Waits, a bounded while, for a condition another thread makes true.
fn eventually(what: &str, mut condition: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(Instant::now() < until, "{what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A run script that waits for the test to release it.
const WAIT_FOR_RELEASE: &str = "while [ ! -e \"$d/release\" ]; do sleep 0.02; done\n";

// ---------------------------------------------------------------------------
// Admission and the opening
// ---------------------------------------------------------------------------

/// **A peer of another uid reads no byte**, section 8.1, and the service's
/// own log names its uid. Perturbation: admit any uid and the open answers.
#[test]
fn a_peer_of_another_uid_reads_no_byte_and_is_logged() {
    let uid = nix::unistd::getuid().as_raw();
    let fx = fixture_with("peer", |s| s.connector_uid = uid.wrapping_add(1));
    let mut client = fx.client();
    assert!(client.closed(), "a refused peer reads no byte");
    eventually("the uid is logged", || {
        fx.log
            .text()
            .contains(&format!(r#""event":"refused_peer","peer_uid":{uid}"#))
    });
}

/// **Each opening refusal of section 8.2**, with its word, and the list a
/// newer connector reads.
#[test]
fn the_opening_refusals_carry_their_words() {
    let fx = fixture("opening");
    let long = format!(
        r#"{{"weaver_admin":{{"version":1,"purpose":"control","agent":"{}"}}}}"#,
        "a".repeat(4100)
    );
    let cases: Vec<(String, &str)> = vec![
        ("not json".into(), "opening_malformed"),
        (
            r#"{"weaver_admin":{"version":1,"purpose":"control","agent":"a1"},"x":{}}"#.into(),
            "opening_malformed",
        ),
        (
            r#"{"weaver_admin":{"version":0,"purpose":"control","agent":"a1"}}"#.into(),
            "opening_malformed",
        ),
        (
            r#"{"weaver_admin":{"version":"1","purpose":"control","agent":"a1"}}"#.into(),
            "opening_malformed",
        ),
        (
            r#"{"weaver_admin":{"version":1,"purpose":"control"}}"#.into(),
            "opening_malformed",
        ),
        (long, "opening_malformed"),
        (
            r#"{"weaver_admin":{"version":2,"purpose":"control","agent":"a1"}}"#.into(),
            "version_unknown",
        ),
        (
            r#"{"weaver_admin":{"version":1,"purpose":"pairing","agent":"a1"}}"#.into(),
            "purpose_unknown",
        ),
        (opening("control", "admin"), "name_refused"),
        (opening("control", "a1-state"), "name_refused"),
        (opening("control", "a b"), "name_refused"),
    ];
    for (line, expected) in cases {
        let mut client = fx.client();
        let _ = client.stream.write_all(format!("{line}\n").as_bytes());
        let value = client.value();
        assert_eq!(word(&value), expected, "{line:.80}");
        assert!(client.closed(), "an opening refusal closes: {line:.80}");
        match expected {
            "version_unknown" => assert_eq!(value["refused"]["versions"], serde_json::json!([1])),
            "purpose_unknown" => {
                assert_eq!(
                    value["refused"]["purposes"],
                    serde_json::json!(["control", "trace", "log"])
                )
            }
            _ => {}
        }
    }
    // A line that ends without its newline.
    let mut client = fx.client();
    client
        .stream
        .write_all(opening("control", "a1").as_bytes())
        .unwrap();
    client.stream.shutdown(std::net::Shutdown::Write).unwrap();
    assert_eq!(word(&client.value()), "opening_malformed");
}

/// **No line within the wait is late**, section 8.2. Perturbation: wait
/// without bound and the read times out instead.
#[test]
fn an_opening_that_does_not_arrive_is_late() {
    let fx = fixture("late");
    let mut client = fx.client();
    let started = Instant::now();
    assert_eq!(word(&client.value()), "opening_late");
    assert!(started.elapsed() < Duration::from_secs(5));
}

/// **At most 8 control connections stand for an agent**, section 8.1, and a
/// closed one frees its place.
#[test]
fn a_ninth_control_connection_is_full_and_a_closed_one_frees_its_place() {
    let fx = fixture("eight");
    let mut held: Vec<Client> = (0..8).map(|_| fx.opened("control")).collect();
    let mut ninth = fx.client();
    ninth.send(&opening("control", "a1"));
    assert_eq!(word(&ninth.value()), "full");
    assert!(ninth.closed());
    drop(held.pop());
    eventually("a place is freed", || {
        let mut client = fx.client();
        client.send(&opening("control", "a1"));
        client.line() == open_line()
    });
}

/// **Past the service's bound, `full` is sent before the opening is read**,
/// section 8.2's table.
#[test]
fn past_the_service_bound_full_comes_before_the_opening() {
    let fx = fixture_with("bound", |s| s.connections_bound = 2);
    let _a = fx.client();
    let _b = fx.client();
    let mut third = fx.client();
    assert_eq!(word(&third.value()), "full");
}

// ---------------------------------------------------------------------------
// The grants
// ---------------------------------------------------------------------------

/// **The grants name the granted verbs in the list's order**, each read with
/// `sudo -n -l` on its fixed line, section 8.3.
#[test]
fn the_grants_name_the_granted_in_the_lists_order() {
    let fx = fixture("grants");
    fx.put("granted", "restore\nshow\nload\n");
    let mut client = fx.opened("control");
    client.send(r#"{"grants":{}}"#);
    assert_eq!(
        client.line(),
        r#"{"grants":{"verbs":["show","load","restore"]}}"#
    );
    let reads: Vec<String> = VERBS.iter().map(|v| fx.read_line(v)).collect();
    assert_eq!(fx.argv(), reads, "eight reads, one per verb, in order");
    assert!(
        fx.log
            .text()
            .contains("Sorry, user may not run stop on a1."),
        "sudo's stderr is kept"
    );
}

/// **A read that does not answer is not a guess**, section 8.3: an exit
/// other than 0 or 1, or the eight not read within the wait.
#[test]
fn a_grant_read_that_does_not_answer_refuses_grants_unread() {
    let fx = fixture("grants-unread");
    fx.put("granted", "show\n");
    fx.put("grant-stop", "2");
    let mut client = fx.opened("control");
    client.send(r#"{"grants":{}}"#);
    assert_eq!(word(&client.value()), "grants_unread");
    std::fs::remove_file(fx.dir.join("grant-stop")).unwrap();
    fx.put("grant-sleep", "5");
    client.send(r#"{"grants":{}}"#);
    let started = Instant::now();
    assert_eq!(
        word(&client.value()),
        "grants_unread",
        "the connection stands"
    );
    assert!(started.elapsed() < Duration::from_secs(4));
}

// ---------------------------------------------------------------------------
// A verb
// ---------------------------------------------------------------------------

/// **The service checks the grant itself on every run-this-verb request**,
/// the operator's first condition and section 8.4: a grant removed after a
/// grants answer is refused at the verb, the read is made in the request on
/// the exact fixed line, and nothing runs. Perturbation: answer the verb from
/// the grants answer, or skip the read, and the removed grant runs.
#[test]
fn the_grant_is_read_in_every_verb_request() {
    let fx = fixture("grant-per-request");
    fx.put("granted", "load\n");
    let mut client = fx.opened("control");
    client.send(r#"{"grants":{}}"#);
    assert_eq!(client.line(), r#"{"grants":{"verbs":["load"]}}"#);
    fx.put("granted", "");
    client.send(&verb_ask("load"));
    let refused = client.value();
    assert_eq!(word(&refused), "not_granted");
    let argv = fx.argv();
    assert_eq!(
        argv.last(),
        Some(&fx.read_line("load")),
        "the verb's own read, after the ask"
    );
    assert!(!argv.contains(&fx.run_line("load")), "nothing ran");
    fx.put("granted", "load\n");
    client.send(&verb_ask("load"));
    assert_eq!(client.line(), r#"{"started":{"run":"0000000000000001"}}"#);
    assert_eq!(word(&client.value()), "", "the run's final line");
    let argv = fx.argv();
    assert_eq!(
        &argv[argv.len() - 2..],
        [fx.read_line("load"), fx.run_line("load")]
    );
}

/// **A grant read that does not answer runs nothing**: an exit other than
/// 0 or 1, and a read past the wait.
#[test]
fn a_verb_whose_grant_is_unread_runs_nothing() {
    let fx = fixture("grant-unread");
    fx.put("grant-load", "2");
    let mut client = fx.opened("control");
    client.send(&verb_ask("load"));
    assert_eq!(word(&client.value()), "grant_unread");
    std::fs::remove_file(fx.dir.join("grant-load")).unwrap();
    fx.put("granted", "load\n");
    fx.put("grant-sleep", "5");
    client.send(&verb_ask("load"));
    assert_eq!(word(&client.value()), "grant_unread");
    assert!(!fx.argv().contains(&fx.run_line("load")));
}

/// **A verb runs its fixed line and carries the root program's object as it
/// printed it**, section 8.4: `started`, then `ran` with the object's member
/// order and number text kept, its standard error, and the principal in no
/// argument (S3) and in the service's own log. Perturbation: re-render the
/// object through a map and the member order changes.
#[test]
fn a_verb_carries_the_root_programs_object_untouched() {
    let fx = fixture("ran");
    fx.put("granted", "load\n");
    fx.put(
        "run-load",
        "printf '{\"z\": 1.50, \"a\": [1, 2], \"s\": \"x y\"}\\n'; echo warn >&2; exit 1\n",
    );
    let mut client = fx.opened("control");
    client.send(&verb_ask("load"));
    assert_eq!(client.line(), r#"{"started":{"run":"0000000000000001"}}"#);
    assert_eq!(
        client.line(),
        r#"{"ran":{"run":"0000000000000001","verb":"load","exit_code":1,"answer":{"z":1.50,"a":[1,2],"s":"x y"},"stderr":"warn\n","stderr_cut":false}}"#
    );
    assert!(
        fx.argv().iter().all(|l| !l.contains("pe-")),
        "the principal reaches no argument"
    );
    assert!(
        fx.log.text().contains(r#""principal":{"kind":"person","person_id":"pe-0123456789abcdef"},"outcome":"ran, exit 1""#),
        "the principal is in the service's own log: {}",
        fx.log.text()
    );
}

/// **Every other end is a `fault`**, and standard error past 16 KiB is cut.
#[test]
fn a_root_program_that_ends_any_other_way_is_a_fault() {
    let fx = fixture("fault");
    fx.put("granted", "load\n");
    let cases = [
        ("exit 2\n", "the root program exited 2"),
        (
            "printf '{} {}'; exit 0\n",
            "its standard output carries more than one value",
        ),
        ("exit 0\n", "its standard output carries no object"),
        (
            "printf '[1]'; exit 0\n",
            "its standard output carries no object",
        ),
        (
            "printf '{\"a\":\"'; head -c 70000 /dev/zero | tr '\\0' a; printf '\"}'; exit 0\n",
            "its standard output is past 64 KiB",
        ),
        (
            "printf '{\"a\":\"\\377\"}'; exit 0\n",
            "its standard output is not UTF-8",
        ),
        ("kill -9 $$\n", "the root program ended by signal 9"),
    ];
    let mut client = fx.opened("control");
    for (n, (script, detail)) in cases.iter().enumerate() {
        fx.put("run-load", script);
        client.send(&verb_ask("load"));
        let id = format!("{:016x}", n + 1);
        assert_eq!(client.line(), format!(r#"{{"started":{{"run":"{id}"}}}}"#));
        assert_eq!(
            client.line(),
            format!(r#"{{"fault":{{"run":"{id}","verb":"load","detail":"{detail}"}}}}"#),
            "{script}"
        );
    }
    fx.put("run-load", "head -c 20000 /dev/zero | tr '\\0' e >&2\n");
    client.send(&verb_ask("load"));
    let _ = client.line();
    let ran = client.value();
    assert_eq!(ran["ran"]["stderr"].as_str().unwrap().len(), STDERR_KEPT);
    assert_eq!(ran["ran"]["stderr_cut"], Value::Bool(true));
}

/// **What stands and what closes**, section 8.2: an unknown ask and an
/// unknown verb stand, a principal of another shape, an ask naming `length`,
/// a line that does not parse and one past 4096 bytes close.
#[test]
fn ask_refusals_that_stand_and_that_close() {
    let fx = fixture("asks");
    let mut client = fx.opened("control");
    client.send(r#"{"pairing":{}}"#);
    assert_eq!(word(&client.value()), "ask_unknown");
    client.send(&verb_ask("reboot"));
    assert_eq!(word(&client.value()), "verb_unknown");
    client.send(RUN_ASK);
    assert!(client.value().get("run").is_some(), "the connection stood");
    for line in [
        r#"{"verb":{"verb":"load","principal":{"kind":"person","person_id":"pe-0123456789ABCDEF"}}}"#.to_string(),
        r#"{"verb":{"verb":"load","principal":{"kind":"server","extra":1}}}"#.to_string(),
        r#"{"grants":{"length":10,"sha256":"00"}}"#.to_string(),
        "{".to_string(),
        format!(r#"{{"run":{{"pad":"{}"}}}}"#, "p".repeat(4100)),
    ] {
        let mut client = fx.opened("control");
        client.send(&line);
        assert_eq!(word(&client.value()), "ask_malformed", "{line:.80}");
        assert!(client.closed(), "{line:.80}");
    }
    assert!(fx.argv().is_empty(), "no refusal read a grant");
}

/// **Two asks in turn on one control connection**, as the connector asks a
/// baseline `run` before its verb: each answered in turn, and the `run` after
/// names the verb's end.
#[test]
fn two_asks_in_turn_on_one_connection() {
    let fx = fixture("in-turn");
    fx.put("granted", "unload\n");
    let mut client = fx.opened("control");
    client.send(RUN_ASK);
    assert_eq!(
        client.line(),
        format!(r#"{{"run":{{"instance":"{INSTANCE}","standing":[],"ended":null}}}}"#)
    );
    client.send(&verb_ask("unload"));
    assert_eq!(client.line(), r#"{"started":{"run":"0000000000000001"}}"#);
    let ran = client.line();
    client.send(RUN_ASK);
    assert_eq!(
        client.line(),
        format!(r#"{{"run":{{"instance":"{INSTANCE}","standing":[],"ended":{ran}}}}}"#)
    );
}

/// **A line while an ask is outstanding closes the connection, and the run
/// goes on**, sections 8.4 and 8.8.
#[test]
fn a_line_while_outstanding_closes_and_the_run_goes_on() {
    let fx = fixture("outstanding");
    fx.put("granted", "load\n");
    fx.put("run-load", WAIT_FOR_RELEASE);
    let mut client = fx.opened("control");
    client.send(&verb_ask("load"));
    assert_eq!(client.line(), r#"{"started":{"run":"0000000000000001"}}"#);
    client.send(RUN_ASK);
    assert_eq!(word(&client.value()), "ask_outstanding");
    assert!(client.closed());
    let mut other = fx.opened("control");
    other.send(RUN_ASK);
    assert_eq!(
        other.line(),
        format!(
            r#"{{"run":{{"instance":"{INSTANCE}","standing":[{{"run":"0000000000000001","verb":"load","started_ms":{CLOCK_MS}}}],"ended":null}}}}"#
        )
    );
    fx.put("release", "");
    eventually("the run ends and is kept", || {
        other.send(RUN_ASK);
        other.value()["run"]["ended"]["ran"]["run"] == "0000000000000001"
    });
}

/// **A close before the grant's read answers starts nothing** (S4).
/// Perturbation: drop the check after the read and the run starts.
#[test]
fn a_close_before_the_grant_answers_starts_nothing() {
    let fx = fixture("close-early");
    fx.put("granted", "load\n");
    fx.put("grant-sleep", "0.4");
    let mut client = fx.opened("control");
    client.send(&verb_ask("load"));
    drop(client);
    eventually("the read was made", || {
        fx.argv().contains(&fx.read_line("load"))
    });
    std::thread::sleep(Duration::from_millis(900));
    assert!(!fx.argv().contains(&fx.run_line("load")), "nothing started");
    let mut other = fx.opened("control");
    other.send(RUN_ASK);
    assert_eq!(
        other.line(),
        format!(r#"{{"run":{{"instance":"{INSTANCE}","standing":[],"ended":null}}}}"#)
    );
}

/// **One run at a time, with the two overlaps** (section 8.4, machine A): a
/// verb beside a run is `busy` naming it, a `show` starts beside anything
/// and is never listed or kept, a `force-unload` starts beside an `unload`,
/// and a third is `busy` naming the `unload`. Perturbation: admit every run
/// and the load starts.
#[test]
fn one_run_at_a_time_with_the_two_overlaps() {
    let fx = fixture("busy");
    fx.put("granted", "show\nload\nunload\nforce-unload\nstop\n");
    fx.put("run-unload", WAIT_FOR_RELEASE);
    let mut first = fx.opened("control");
    first.send(&verb_ask("unload"));
    assert_eq!(first.line(), r#"{"started":{"run":"0000000000000001"}}"#);
    let mut second = fx.opened("control");
    second.send(&verb_ask("load"));
    assert_eq!(
        second.line(),
        r#"{"refused":{"word":"busy","detail":"another run of the agent stands","run":"0000000000000001"}}"#
    );
    second.send(&verb_ask("show"));
    assert_eq!(second.line(), r#"{"started":{"run":"0000000000000002"}}"#);
    assert!(second.value().get("ran").is_some(), "a show runs beside");
    second.send(&verb_ask("force-unload"));
    assert_eq!(second.line(), r#"{"started":{"run":"0000000000000003"}}"#);
    let forced = second.line();
    let mut third = fx.opened("control");
    third.send(&verb_ask("stop"));
    assert_eq!(third.value()["refused"]["run"], "0000000000000001");
    third.send(RUN_ASK);
    assert_eq!(
        third.line(),
        format!(
            r#"{{"run":{{"instance":"{INSTANCE}","standing":[{{"run":"0000000000000001","verb":"unload","started_ms":{CLOCK_MS}}}],"ended":{forced}}}}}"#
        ),
        "the force-unload ended and is kept, the show never"
    );
    fx.put("release", "");
    let ran = first.line();
    third.send(RUN_ASK);
    assert_eq!(
        third.line(),
        format!(r#"{{"run":{{"instance":"{INSTANCE}","standing":[],"ended":{ran}}}}}"#)
    );
}

/// **A root program that cannot be started is `not_started`**, and the run
/// leaves `standing` with nothing kept as `ended`.
#[test]
fn a_root_program_that_does_not_start_is_not_started() {
    let fx = fixture("not-started");
    fx.put("granted", "load\n");
    fx.put("vanish", "");
    let mut client = fx.opened("control");
    client.send(&verb_ask("load"));
    assert_eq!(word(&client.value()), "not_started");
    client.send(RUN_ASK);
    assert_eq!(
        client.line(),
        format!(r#"{{"run":{{"instance":"{INSTANCE}","standing":[],"ended":null}}}}"#)
    );
    assert!(fx.log.text().contains(r#""outcome":"not_started""#));
}

/// **A run goes on when its connection closes**, and a `run` on a new
/// connection names it standing and then ended (S4).
#[test]
fn a_run_goes_on_after_its_connection_closes() {
    let fx = fixture("goes-on");
    fx.put("granted", "load\n");
    fx.put("run-load", WAIT_FOR_RELEASE);
    let mut client = fx.opened("control");
    client.send(&verb_ask("load"));
    assert_eq!(client.line(), r#"{"started":{"run":"0000000000000001"}}"#);
    drop(client);
    let mut other = fx.opened("control");
    other.send(RUN_ASK);
    assert_eq!(
        other.value()["run"]["standing"][0]["run"],
        "0000000000000001"
    );
    fx.put("release", "");
    eventually("ended names it", || {
        other.send(RUN_ASK);
        other.value()["run"]["ended"]["ran"]["answer"]["verb"] == "load"
    });
}

/// **A descendant holding the root program's output does not hold its
/// answer**: the object it printed before its exit is carried.
#[test]
fn a_descendant_holding_the_pipes_does_not_hold_the_answer() {
    let fx = fixture("descendant");
    fx.put("granted", "load\n");
    fx.put("run-load", "sleep 30 &\n");
    let mut client = fx.opened("control");
    client.send(&verb_ask("load"));
    let _ = client.line();
    let started = Instant::now();
    assert!(client.value().get("ran").is_some());
    assert!(started.elapsed() < Duration::from_secs(5));
}

// ---------------------------------------------------------------------------
// The trace
// ---------------------------------------------------------------------------

/// **The trace is spliced to the relay**, section 8.5: `open` only once the
/// dial connected, the connector's request forwarded as sent (here in the
/// same write as its opening line), the relay's bytes back unchanged, and the
/// relay's close closes the connector's side.
#[test]
fn the_trace_is_spliced_to_the_relay() {
    let fx = fixture("trace");
    let door = fx.dir.join("coordination/weaver.run/a1/trace.sock");
    let relay = UnixListener::bind(&door).unwrap();
    let request = br#"{"from":"start"}"#;
    let mut client = fx.client();
    let mut first = format!("{}\n", opening("trace", "a1")).into_bytes();
    first.extend_from_slice(request);
    first.push(b'\n');
    client.stream.write_all(&first).unwrap();
    let (mut accepted, _) = relay.accept().unwrap();
    accepted
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    assert_eq!(client.line(), open_line());
    let mut got = vec![0_u8; request.len() + 1];
    accepted.read_exact(&mut got).unwrap();
    assert_eq!(
        &got[..request.len()],
        request,
        "the request reaches the relay as sent"
    );
    let header = b"{\"header\":1}\n{\"event\":\"a\"}\n";
    accepted.write_all(header).unwrap();
    assert_eq!(client.line(), "{\"header\":1}");
    assert_eq!(client.line(), "{\"event\":\"a\"}");
    drop(accepted);
    assert!(
        client.closed(),
        "the relay's close closes the connector's side"
    );
}

/// **No relay to dial is `relay_unreachable`**, in place of `open`.
#[test]
fn no_relay_is_relay_unreachable() {
    let fx = fixture("no-relay");
    let mut client = fx.client();
    client.send(&opening("trace", "a1"));
    assert_eq!(word(&client.value()), "relay_unreachable");
    assert!(client.closed());
}

// ---------------------------------------------------------------------------
// The log
// ---------------------------------------------------------------------------

/// **The log's lines as appended**, section 8.6: no history before `open`,
/// an object carried as written with `through_service` judged from the line
/// alone, a line that is no object as text, one past 8 KiB as its length, and
/// a truncation and a replacement as discontinuities. Perturbation: judge
/// `through_service` from `uid` alone and the relay's line reads false.
#[test]
fn the_log_carries_lines_as_appended() {
    let fx = fixture("log");
    let uid = nix::unistd::getuid().as_raw();
    fx.append("{\"history\":1}\n");
    let mut client = fx.opened("log");
    fx.append(&format!(
        "{{\"wall_ms\":1,\"uid\":{uid},\"verb\":\"load\"}}\n"
    ));
    assert_eq!(
        client.line(),
        format!(
            r#"{{"log":{{"line":{{"wall_ms":1,"uid":{uid},"verb":"load"}},"through_service":true}}}}"#
        )
    );
    fx.append("{\"uid\":4242,\"verb\":\"show\"}\n");
    assert_eq!(
        client.line(),
        r#"{"log":{"line":{"uid":4242,"verb":"show"},"through_service":false}}"#
    );
    fx.append(&format!(
        "{{\"actor\":\"relay\",\"uid\":4242,\"peer_uid\":{uid}}}\n"
    ));
    assert_eq!(client.value()["log"]["through_service"], Value::Bool(true));
    fx.append("not an object\n");
    assert_eq!(
        client.line(),
        r#"{"log":{"text":"not an object","through_service":false}}"#
    );
    fx.append(&format!("{}\n", "o".repeat(9000)));
    assert_eq!(client.line(), r#"{"log":{"oversized":9000}}"#);
    std::fs::write(fx.log_path(), "").unwrap();
    eventually("the truncation is seen", || {
        std::fs::metadata(fx.log_path()).unwrap().len() == 0
    });
    assert_eq!(client.line(), r#"{"log":{"discontinuity":"truncated"}}"#);
    fx.append("{\"after\":1}\n");
    assert_eq!(client.value()["log"]["line"]["after"], 1);
    let newer = fx.dir.join("territory").join("admin.log.new");
    std::fs::write(&newer, "{\"replaced\":1}\n").unwrap();
    std::fs::set_permissions(&newer, std::fs::Permissions::from_mode(0o640)).unwrap();
    std::fs::rename(&newer, fx.log_path()).unwrap();
    assert_eq!(client.line(), r#"{"log":{"discontinuity":"replaced"}}"#);
    assert_eq!(
        client.value()["log"]["line"]["replaced"],
        1,
        "the new file read from its start"
    );
}

/// **The newest log connection replaces the older**, and **a byte from the
/// connector is malformed**, section 8.6.
#[test]
fn a_newer_log_connection_closes_the_older_and_a_byte_is_malformed() {
    let fx = fixture("log-newer");
    let mut older = fx.opened("log");
    let mut newer = fx.opened("log");
    assert!(older.closed(), "the older is closed");
    newer.send("x");
    assert_eq!(word(&newer.value()), "ask_malformed");
    assert!(newer.closed());
}

/// **A log that does not open or judge is `log_unread`**: absent, a link,
/// a second link, and an agent with no root.
#[test]
fn an_unjudged_log_is_log_unread() {
    let fx = fixture("log-unread");
    let path = fx.log_path();
    let aside = fx.dir.join("admin.log.aside");
    std::fs::rename(&path, &aside).unwrap();
    let refused = |agent: &str| {
        let mut client = fx.client();
        client.send(&opening("log", agent));
        word(&client.value()).to_string()
    };
    assert_eq!(refused("a1"), "log_unread", "absent");
    std::os::unix::fs::symlink(&aside, &path).unwrap();
    assert_eq!(refused("a1"), "log_unread", "a symbolic link");
    std::fs::remove_file(&path).unwrap();
    std::fs::hard_link(&aside, &path).unwrap();
    assert_eq!(refused("a1"), "log_unread", "a second link");
    assert_eq!(refused("a2"), "log_unread", "no root");
}

/// **A connector that does not read is closed past 1 MiB unsent**, section
/// 8.6. Perturbation: no bound and the connection stays open.
#[test]
fn a_log_connector_that_does_not_read_is_closed() {
    let fx = fixture("log-behind");
    let mut client = fx.opened("log");
    let line = format!("{{\"pad\":\"{}\"}}\n", "b".repeat(4000));
    fx.append(&line.repeat(1024));
    std::thread::sleep(Duration::from_millis(800));
    let mut all = Vec::new();
    client.reader.read_to_end(&mut all).unwrap();
    assert!(
        all.len() < 2 * 1024 * 1024,
        "closed with most of 4 MiB unsent: {}",
        all.len()
    );
}

// ---------------------------------------------------------------------------
// The shared transcript
// ---------------------------------------------------------------------------

/// The exchanges of section 8 as the service's tests make them, one line
/// each, which both repositories keep byte for byte.
const TRANSCRIPT: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/service-transcript.ndjson"
);

struct Recorder {
    lines: Vec<String>,
}

impl Recorder {
    fn put(&mut self, case: &str, from: &str, line: &str) {
        self.lines.push(object(&[
            ("case", js(case)),
            ("from", js(from)),
            ("line", js(line)),
        ]));
    }
    fn ask(&mut self, case: &str, client: &mut Client, line: &str) {
        self.put(case, "connector", line);
        client.send(line);
    }
    fn answer(&mut self, case: &str, client: &mut Client) -> String {
        let line = client.line();
        self.put(case, "service", &line);
        line
    }
}

/// **The transcript is the service's own**: each exchange run against the
/// service and recorded, then compared with the fixture byte for byte.
/// `WEAVER_ADMIN_SERVICE_TRANSCRIPT=write` writes it instead.
#[test]
fn the_transcript_is_the_services_own() {
    let fx = fixture("transcript");
    let uid = nix::unistd::getuid().as_raw();
    let mut r = Recorder { lines: Vec::new() };
    for (case, line) in [
        (
            "opening refused: version",
            r#"{"weaver_admin":{"version":2,"purpose":"control","agent":"a1"}}"#.to_string(),
        ),
        (
            "opening refused: purpose",
            r#"{"weaver_admin":{"version":1,"purpose":"pairing","agent":"a1"}}"#.to_string(),
        ),
        ("opening refused: name", opening("control", "admin")),
        ("opening refused: relay", opening("trace", "a1")),
    ] {
        let mut client = fx.client();
        r.ask(case, &mut client, &line);
        r.answer(case, &mut client);
    }
    fx.put("granted", "show\nload\nunload\n");
    fx.put(
        "run-load",
        "printf '{\"lifecycle-answer\":{\"agent\":\"a1\",\"state\":\"Serving\"}}\\n'; exit 0\n",
    );
    fx.put("run-unload", WAIT_FOR_RELEASE);
    let mut c = fx.client();
    r.ask("control", &mut c, &opening("control", "a1"));
    r.answer("control", &mut c);
    r.ask("grants", &mut c, r#"{"grants":{}}"#);
    r.answer("grants", &mut c);
    r.ask("baseline", &mut c, RUN_ASK);
    r.answer("baseline", &mut c);
    r.ask("verb ran", &mut c, &verb_ask("load"));
    r.answer("verb ran", &mut c);
    r.answer("verb ran", &mut c);
    r.ask("verb not granted", &mut c, &verb_ask("stop"));
    r.answer("verb not granted", &mut c);
    r.ask("verb unknown", &mut c, &verb_ask("reboot"));
    r.answer("verb unknown", &mut c);
    r.ask("ask unknown", &mut c, r#"{"pairing":{}}"#);
    r.answer("ask unknown", &mut c);
    r.ask(
        "verb standing",
        &mut c,
        r#"{"verb":{"verb":"unload","principal":{"kind":"server"}}}"#,
    );
    r.answer("verb standing", &mut c);
    let mut d = fx.client();
    r.ask("busy", &mut d, &opening("control", "a1"));
    r.answer("busy", &mut d);
    r.ask("busy", &mut d, &verb_ask("load"));
    r.answer("busy", &mut d);
    r.ask("run standing", &mut d, RUN_ASK);
    r.answer("run standing", &mut d);
    fx.put("release", "");
    r.answer("verb standing", &mut c);
    r.ask("run ended", &mut d, RUN_ASK);
    r.answer("run ended", &mut d);
    let mut l = fx.client();
    r.ask("log", &mut l, &opening("log", "a1"));
    r.answer("log", &mut l);
    fx.append(&format!("{{\"wall_ms\":1,\"command\":\"weaver-admin load a1\",\"uid\":{uid},\"verb\":\"load\",\"agent\":\"a1\",\"outcome\":\"ready\"}}\n"));
    let line = l.line().replace(&format!("\"uid\":{uid}"), "\"uid\":992");
    r.put("log", "service", &line);
    fx.append("a line that is no object\n");
    r.answer("log", &mut l);
    fx.append(&format!("{}\n", "o".repeat(9000)));
    r.answer("log", &mut l);
    std::fs::write(fx.log_path(), "").unwrap();
    r.answer("log", &mut l);
    let mut text = r.lines.join("\n");
    text.push('\n');
    if std::env::var("WEAVER_ADMIN_SERVICE_TRANSCRIPT").as_deref() == Ok("write") {
        std::fs::write(TRANSCRIPT, &text).unwrap();
    }
    let kept = std::fs::read_to_string(TRANSCRIPT).expect("the shared transcript");
    assert_eq!(
        kept, text,
        "the transcript the service makes is the one kept"
    );
}
