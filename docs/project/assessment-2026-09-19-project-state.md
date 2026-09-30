# Independent Assessment of WeaverTools

**Version:** v1.0, 2026-09-19.
**Parent:** WeaverTools Working Process.

## 1. Overall judgment

WeaverTools has a credible technical foundation and a coherent purpose. Its strongest
engineering is inside individual components and contracts. Its weakest engineering is in
proving that the assembled system consistently delivers the intended operator experience
and supports the conclusions drawn from its records.

The recommendation is to continue with the architecture and concentrate the next
development phase on operational completeness, provenance, and repeatable acceptance
evidence. Expanding capability before closing those gaps would increase the existing
maintenance burden and make experimental results harder to interpret.

HADES materially changes the assessment of the process. The document corpus produces
navigable engineering relationships, and substantial code and tests enforce the claims
those relationships connect. This is a useful engineering asset. It does not, by itself,
establish that the complete application starts, that a claimed deadline bounds the whole
operation, or that recorded provenance identifies the code executed on every turn. Those
are the consequential gaps found in this survey.

This report separates facts found, advice offered, and requests to reopen decisions.
Recommendations are advisory. Filing this assessment does not amend a charter, Spec,
contract, or standing ruling.

## 2. Scope, method, and evidence limits

The review was a repository-wide survey with focused code inspection, HADES traversal
and semantic retrieval, host-side tests, an isolated database run, and a bounded
subprocess probe. It was performed by one Codex review session, without delegated
reviewers. It was not a line-by-line verification of every source unit or an adversarial
deployment security audit.

The original assessment examined commit `c30b4b9aa4ee14d0cd4e958fb1572c46c75b6049` and
HADES database `WeaverTools_v5`. The graph was treated as a retrieval and relationship
source. Current files were used to resolve discrepancies. Inspection covered the shared
vocabulary, trace and diagnostic recorders, harness lifecycle and tool execution, gate,
SPU, state engines, analysis readers, web composition, deployment script, and process
gates.

Before publication, `origin/main` had advanced by four commits to `f03142a`. That
publication baseline includes PR #638, which fixes the PostgreSQL index-naming defect
present in the assessed snapshot, and PR #636, which expands the census. Section 7
distinguishes those changes from the original results. Earlier test and graph counts
below belong to the original assessment baseline and are not represented as fresh
measurements of every later commit.

No application source or governing document was changed during the survey. Test output,
the disposable PostgreSQL cluster, and the shell probe were held under `/tmp`. The
isolated database used a private Unix socket and was stopped after the checks. No
existing database was used, no deployment script was run, and no live agent was
reconfigured. The later instruction to publish this report and the previously requested
`AGENTS.md` authorized the documentation commit separately.

CUDA and multi-GPU experiments were not independently repeated. The absence of such a
run is not evidence of absent hardware or a failed device implementation. The current
machine guidance names Olympus as the device-validation lane. Repository statements
about prior device measurements are attributed as recorded evidence, not presented as
measurements performed by this review.

The raw review logs remain local scratch artifacts. The findings, commands, observed
outputs, reproduction conditions, and scope limits are recorded here so the report does
not depend on those temporary paths remaining available. Test totals are reported
executions, not counts of unique properties or exhaustive coverage.

## 3. Verification results

| Check at the original assessment baseline | Result | Qualification |
|---|---|---|
| `cargo test --workspace --locked --offline` | 632 reported passes, zero failures | Includes conditional tests that can return without exercising their external dependency |
| Workspace Clippy with warnings denied | Passed | Default feature set |
| `cargo fmt --all -- --check` | Passed | Formatting only |
| `process/gates/lock.sh` | Passed | Resolution matched the committed lockfile |
| Harness/state tests with Python worker and PostgreSQL features | 133 reported passes, zero failures | Overlaps the default suite and does not establish live PostgreSQL state-engine behavior |
| Census script fixture suite | 33 passed | Original census implementation |
| Grounds-parity script fixture suite | 13 passed | Tests of the checker, not a new graph-wide adjudication |
| Web library tests without PostgreSQL | 23 reported passes | 19 returned without exercising their database paths |
| Web library tests against isolated PostgreSQL | All 23 passed | Database paths executed, with one test thread |
| Web binary startup against the isolated database | Failed, exit 1 | `relation "participants" does not exist` |
| Gate shell probe | 50 ms requested, 2,009 ms elapsed | A detached descendant retained the output pipes |

The default suite exercised real host-side GGUF model loading. It was not entirely a
mock-based run. That evidence does not extend to CUDA, multi-GPU execution, all
supported model families, or the deployed operating-system boundary.

The first workspace test attempt failed because the execution sandbox denied temporary
Unix-socket binding with `EPERM`. Repeating outside that restriction passed. Those
initial failures were environmental, not counted as product failures.

The first disposable database attempt omitted an explicit connection username and failed
before application validation. After the isolated cluster and URL used the explicit
`weaver_assessment` role, all web tests passed and the application startup failure
reproduced. The missing `participants` finding rests on that corrected run.

The original census passed against its baseline with 38 sources without headers, 28
uncited perturbation assertions, and 11 documents without enforcement tables. These are
recorded evidence gaps. They are not proof of that many behavioral failures or missing
tests. No new census defects were reported in that run.

## 4. Strengths worth preserving

### 4.1 The architecture serves a specific investigative purpose

The trace-first design gives the project a clear reason to exist. The framework is an
instrument for investigating agent behavior, and its boundaries establish the conditions
under which observations can be interpreted. Custody separation, explicit authorship,
reproducible inputs, and typed failures follow from that purpose rather than from a
generic framework template.

Treating the model as one component of the agent is useful. It leaves room to
investigate memory, tools, and loop policy without attributing every failure to the
decoder. The local-first arrangement reduces uncontrolled variables and gives the
project a coherent operating envelope. It should be judged against that purpose rather
than against the deployment flexibility of a general network service.

### 4.2 Important guarantees are expressed in code

The recorder takes descriptors rather than sink paths. The harness owns event
authorship. Tool-result grants have a restricted construction path. Lifecycle states
carry their resources, and refusal cases are represented in types. Compile-fail tests
watch several forbidden constructions rather than relying on comments to discourage
them.

The [tool-result grant][tool-grant] is a strong example. Ordinary serialized data cannot
manufacture the authority represented by that type. The [diagnostic
gate][diagnostic-gate] distinguishes certified, diverged, abandoned, and unfinished
work. These distinctions are fundamental to the instrument's credibility and should
survive any effort to simplify the process.

### 4.3 The tests include meaningful boundary exercises

The suite includes real processes, sockets, descriptor transfer, refusal paths,
compile-time checks, and host model execution. Perturbation-oriented tests ask whether
removing the property changes the result. That is stronger evidence than asserting only
a nominal successful response.

The implementations also show attention to failure details that are often missed:
descriptor inheritance, lost wakeups, whole-event framing, partial writes, child exit
handling, and whether a failure is attributable to a component or to the channel itself.
The shell finding below does not erase the care already present in its supervision code.

### 4.4 The graph makes the document corpus actionable

HADES held 418 assertions, 504 code-to-assertion links, 4,868 code symbols, and 3,431
call edges at the time queried. The review followed `types-denial-precedes-permission`
to its declaring Spec, owning crate, implementation, and tests. The two indexed code
hashes matched the checkout.

This makes the process investment useful for impact analysis, onboarding, and review. A
reviewer can begin with a claim and locate its implementation and intended instrument.
The next opportunity is to connect those relationships to execution receipts and exact
build conditions, not merely to add more prose.

### 4.5 Scientific interpretation is treated as an engineering concern

Capture comparison checks provenance and cardinality before comparing values. Empty or
incompatible evidence is distinguished from agreement. Analysis gates readings on replay
outcomes. Readout failures have an explicit fault case rather than silently becoming an
unelected observation.

The [reproducibility account][reproducibility] often states its experimental scope
carefully. The [readout implementation][readout] names both the backend mechanic and the
device measurement it says was taken. These are useful foundations for a research
instrument, provided claims remain tied to the conditions that demonstrated them.

## 5. Current state by subsystem

| Area | Assessment | Principal limitation |
|---|---|---|
| Shared types and traits | Relatively mature foundation with useful compile-time enforcement | Interface evolution still requires coordinated changes across consumers |
| Trace and diagnostic recording | Strong design and substantial tests | Long-duration resource behavior and stalled-sink behavior need more acceptance evidence |
| Harness and lifecycle | Substantial working implementation | Tool cancellation and Python provenance have important gaps |
| Gate and shell execution | Real boundary enforcement and bounded output capture | The deadline does not bound detached descendants and output draining |
| SPU | Substantial integration with real inference backends | Confidence remains feature-, artifact-, and device-specific |
| State | SQLite has stronger behavioral evidence | PostgreSQL live-server parity remains incomplete after the index-name fix |
| Analysis | Careful comparability and certification design | Malformed-input accounting deserves reconsideration |
| Web | New schema and read components pass database tests | Current startup still invokes retired conversation services |
| Internal calculator | Implemented pure component | Intended intervention wiring is not complete |

This is an advanced development system with significant working machinery. It is not yet
a uniformly complete operator-facing research product. The strongest local components
should not be described as immature merely because integration is unfinished, and their
maturity should not be used to imply that the whole application is ready.

## 6. Facts found and corrective priorities

### F1. Web startup and the current schema disagree

**Priority: high. Evidence: reproduced. Disposition: open at publication.**

The [server startup][web-main] connects the current store and then invokes provider and
role reconciliation from the retired conversation implementation. Role reconciliation
queries `participants`, which the current migrations do not create. The call runs even
with no configured providers or administrators.

Against an isolated PostgreSQL instance, all 23 web library tests passed. The server
binary then exited with `relation "participants" does not exist`. The [conversation
module][conversation] documents that its statements target tables the schema no longer
creates, but startup still reaches that module unconditionally.

This is a known architectural transition with a reproduced runtime consequence. Passing
component tests currently does not establish that the entry point starts. The new read
path is supported by executed tests, while the composition root remains coupled to the
retired application.

**Advice:** finish the composition-root transition and add a startup acceptance check
against a newly initialized database. Completion means the process reaches its intended
usable surface, with a working way to obtain the required session, rather than only
completing migrations or building a router in isolation. Avoid investing in retired
conversation behavior unless it is required to remove the coupling.

### F2. The shell clock does not bound the complete operation

**Priority: high. Evidence: reproduced. Disposition: open at publication.**

The probe requested a 50 ms clock for a shell command that started a two-second `sleep`
in a new session and waited for it. The operation returned after 2,009 ms with `Killed {
partial: None }`.

The [executor][shell-executor] kills the shell's process group and joins the stdout and
stderr reader threads. A descendant that has changed session or process group can
survive that kill while retaining the pipes. Joining the readers waits for that
descendant. A longer-lived descendant can hold the operation far beyond the caller's
clock.

The [harness execution wait][harness-execute] awaits the gate response without
independently servicing the coordination path inside that wait. The finding therefore
concerns turn liveness and cancellation, not only imperfect subprocess cleanup. The
probe demonstrated the excessive wait. It did not independently execute an operator stop
against a live agent.

**Advice:** specify and implement a bound over execution, descendant containment, output
draining, and cancellation together. Test ordinary background children, detached
sessions, retained pipe descriptors, and operator stop during a tool call. A
process-group kill alone cannot substantiate a guarantee over every descendant.

### F3. Python hot reload can invalidate recorded composer provenance

**Priority: high for experimental interpretation. Evidence: code inspection.
Disposition: open at publication.**

The [Python worker entry point][pyworker-main] hashes its loop file once before entering
service. The [crossing][pyworker-loop] rereads and compiles that file on every turn. An
edit can change the code executed while the record continues carrying the earlier
composer identity.

Hot reload is intentional and is a likely workflow during agent experiments. This is
therefore a provenance gap on an advertised iteration path, not a concern requiring an
unsupported mode. It is confined to that Python path in this finding. No corresponding
failure of compiled loops was demonstrated.

**Advice:** identify the bytes executed at each crossing, or turn a code change into an
explicit run or experiment boundary. The hash and execution must refer to the same read
of the bytes. Hashing more often without binding the hash to the executed source leaves
a race. Failures and fallback execution also need identifiable provenance.

### F4. Passing test totals conceal absent execution

**Priority: high for assurance. Evidence: reproduced and inspected. Disposition: open at
publication.**

The [web database test helper][web-test-helper] returns when `DATABASE_URL` is absent.
The default successful-test output hides the message printed before that return.
Nineteen of the 23 web tests did this in the default run. With a disposable database
provided, all 23 passed.

Some SPU tests depend on feature flags, model artifacts, and devices. Conditional
returns and feature-disabled suites are different from properties exercised
successfully. The Python worker target reported zero tests of its own in the
optional-feature run. The result is a reporting gap even where the skipped behavior
later proves sound.

No checked-in GitHub Actions workflow was found. External automation may exist, so this
is not a claim that no automation runs. The tree does not itself provide a visible
complete acceptance pipeline. The deployment script tests a narrower package selection
than the full workspace and does not substitute for one.

**Advice:** define named validation profiles for core host behavior, databases, Python
loops, CPU inference, device inference, and application/deployment acceptance. A run
intended to certify a profile should fail when its prerequisites are absent. Developer
convenience can remain permissive, but certification must distinguish passed, skipped,
unavailable, and not selected. Record feature flags and fixture identities beside
results.

### F5. PostgreSQL behavioral evidence trails SQLite

**Priority: medium. Evidence: code inspection and feature tests. Disposition: naming
defect fixed upstream, live-server assurance gap remains.**

At the original assessment baseline, index names were derived from hex-encoded elected
key paths without respecting PostgreSQL's identifier-width limit. Distinct names could
be truncated to the same identifier, and `CREATE INDEX IF NOT EXISTS` could silently
retain only one index. The code itself recorded the defect as issue #618. This review
did not establish data corruption from it.

PR #638, present at the publication baseline, replaces that construction with bounded
naming and refusal, reads the server's identifier limit, and adds nine feature-gated
tests. The old collision defect is not presented here as still open. Section 7 records
the publication check.

The remaining gap is behavioral parity. The [updated PostgreSQL module][postgres-engine]
says its tests do not construct a live `Postgres`, watch the catalog, or enforce the
session predicate. Compiling the feature and testing generated statements cannot
establish isolation, transaction behavior, and indexing against a running server. The
disposable web database run tested `weaver-web`, not this state engine.

**Advice:** run the same behavioral contract against both engines, including
cross-session reads, transaction rollback, elected indexes, preload retirement, and
reopening a store. A common trait establishes a common interface, not equivalent
behavior.

### F6. Graph and status freshness require explicit interpretation

**Priority: medium. Evidence: hash comparison and source inspection. Disposition:
process improvement.**

At the original baseline, 183 of 188 indexed code files and 38 of 46 indexed documents
matched the checkout byte-for-byte. No indexed paths were missing. A differing hash may
reflect comments rather than a changed guarantee. It still prevents an unqualified claim
that every graph result describes the current checkout.

Status prose also trails implementation. The [unfinished-work register][unfinished]
describes device-tap neutrality as owed, while the readout implementation names a
completed measurement and its instrument. Replay implementation has advanced beyond
portions of the older status narrative. Staleness can understate progress as well as
overstate readiness.

**Advice:** accompany graph-backed results with their source snapshot and freshness
status. Attach execution evidence to commits, features, artifacts, and devices. Derive
capability status from those records where possible, and retain one authoritative
location for unresolved work. Existing census backlog metrics should stay distinct from
claims that behavior is missing or incorrect.

### F7. Long-running behavior needs an explicit operating envelope

**Priority: medium before longer-lived operation. Evidence: implementation inspection.
Disposition: risk to measure, not a reproduced failure.**

The serving [working structure][working-structure] retains an append-only in-memory
record. Decoder context flushing does not bound that accumulated record. The [trace
writer][trace-writer] applies backpressure at queue capacity, which is a deliberate
completeness policy. A live sink that stops consuming is different from a sink that
immediately returns an error.

The design needs measured limits for resident memory versus event volume, readout
volume, sustained turn count, collector stalls, state-service interruption, and
cancellation under those conditions. A bounded queue does not by itself bound total
memory, response latency, or recovery time.

**Advice:** establish soak and fault-injection acceptance runs before expanding agent
lifetime or concurrency. Decide from their results whether the current retention and
backpressure policy needs a change. The review does not recommend weakening trace
custody or silently dropping events to make a benchmark pass.

## 7. Changes already landed before publication

Four upstream commits arrived after the original survey. They were inspected before the
report was written onto main. Their changes were fast-forwarded rather than overwritten.

| Upstream change | Effect on this assessment |
|---|---|
| `7c7497f`, PR #634 | Clarifies charter/domain context. Original graph freshness figures remain historical measurements |
| `7a7b688`, PR #638 | Fixes the PostgreSQL index-name collision mechanism and adds nine tests. F5 retains only the live-server evidence gap as open |
| `f234449`, PR #636 | Expands the census to supported units. Original census counts and fixture totals are not described as current totals |
| `f03142a` | Corrects local CUDA-toolchain guidance and preserves the Olympus device lane. This report makes no absent-compiler claim |

The three principal open findings concern files unchanged by those commits: web startup
composition, shell execution, and Python provenance. Their source-level mechanisms
therefore remain present at the publication baseline. The original reproductions were
not described as new runs against the later commit.

The publication check ran `cargo test -p weaver-state --features postgres --lib
--locked --offline`: 22 tests passed, including the nine new PostgreSQL naming tests.
The updated census fixture suite passed 35 tests. The census reported no new defects,
with its expanded baseline carrying 42 sources without headers, 28 uncited
perturbations, and 11 documents without enforcement tables. These checks validate the
specific intervening changes. They do not extend the original run into a new full
workspace or device-validation result.

## 8. Requests to reopen decisions

### R1. Identity triggers should follow authority as well as network placement

The web identity model is deliberately a claim rather than authentication. A person can
claim a configured administrator's name under the current design. The [web
charter][web-charter] names that limitation, and the connector link depends heavily on
placement and trust. This is not filed as a violation of the present contract.

The request is to reconsider whether reach beyond the chosen network is a sufficient
trigger for authentication. Processes and people inside that network may still be
outside the intended operator authority. Lifecycle operations and experimental records
warrant an explicit deployment trust envelope. A private or loopback listener limits
reach, but does not establish who an accessible peer is.

### R2. Malformed records should have visible evidentiary consequences

The [analysis parser][analysis-parser] intentionally skips malformed lines, and a test
preserves the behavior. That is a settled policy rather than an accidental missing error
branch.

An unknown valid event and a damaged event are different evidentiary situations. The
request is to preserve forward compatibility while reporting rejected lines, sequence
gaps, and incomplete input. A permissive inspection mode could coexist with stricter
certification. This report does not claim that all current readings are invalid or that
the existing parser violates its Spec.

### R3. Locality should not rest on the impossibility of replication

The [apex rationale][apex] argues that unique agent state cannot be replicated
regardless of architecture. Uniqueness of logical identity does not establish an
impossibility of replicating stored state. The choices of authority, recovery,
consistency, and simultaneous execution are separate questions.

The request is to strengthen that rationale. Locality remains defensible for controlled
measurement, bounded scope, and operating simplicity. This is not a recommendation to
build a distributed framework, nor does it reopen the substrate choice by itself.

## 9. Scientific readiness and the unfinished deliverable

The scientific ambition is stronger than the demonstrated generality of the evidence.
That is appropriate during development if scope stays explicit. A reproduced
configuration does not establish reproducibility across configurations. A neutral
observational tap does not establish that an intervention is useful. A coherent latency
argument does not establish that latency caused better agent behavior.

The latency/agency hypothesis needs controlled measurements separating transport,
serialization, inference, scheduling, and orchestration. The code pays several of those
costs together. Optimizing one without measuring the others could improve the apparatus
while leaving the target behavior unchanged.

The project has not yet completed its own entire definition of done. The calculator
exists, but the [current calculator account][calculator] says its cut-and-recompute
wiring is unfinished and its firing remains gated. The apex names a deterministic
replacement mechanic in its deliverable. Presence of the calculator crate or adjacent
replay machinery is not evidence that the complete mechanic has run.

The next scientific milestone should be one repeatable workflow: declare an agent, run a
task, inspect the record, reproduce it, change one declared variable, and compare the
outcome. A reader who did not build the system should be able to carry out that workflow
and identify the boundaries of the resulting claim.

## 10. Process assessment and recommended order of work

The process is effective at asking whether an implementation satisfies a named clause.
It is less consistent at establishing whether an operator can complete a whole task with
the assembled program and whether the result supports the intended inference. F1 is the
clearest example: passing database tests coexist with an unusable startup path.

The long explanations of previous failures are useful where they preserve a safety
argument about descriptors, concurrency, or ownership. Elsewhere, repeated historical
explanations make the active rule harder to find. HADES can reduce retrieval cost, but
it cannot decide which stale statement is authoritative without maintained relationships
and freshness information.

The human adjudicator is another scaling limit. Architectural decisions belong there,
but routine acceptance should become mechanical and reproducible. The recommendation is
to retain contracts and the graph while reducing independently maintained descriptions
of current status. The evidence chain should reach from a claim to its instrument, its
execution, and the exact conditions under which it ran.

| Order | Outcome | Acceptance evidence |
|---|---|---|
| 1 | Make the present system operationally whole | Current web binary starts on a fresh schema, detached shell children cannot defeat the deadline, Python execution matches recorded provenance |
| 2 | Make validation explicit and attributable | Required profiles fail on missing prerequisites, results identify build/features/artifacts/devices, graph answers identify their source snapshot |
| 3 | Complete one operator-facing experimental workflow | A new reader can execute and reproduce a documented experiment without relying on the original author's unwritten knowledge |
| 4 | Expand capability on that foundation | Intervention and memory work inherit a tested measurement apparatus rather than changing it and the subject at the same time |

The first phase should not become a general cleanup campaign. Fix the reproduced
composition and liveness defects, resolve the provenance gap, and put acceptance checks
at the boundaries that missed them. Run backend parity and long-duration checks against
the scope the project intends to support. Reconsider architectural decisions only where
measurements or explicit arguments justify it.

The assessment supports continuing the project. Confidence is highest in its purpose,
foundational architecture, and local enforcement. Confidence is lower in assembled
application readiness, long-duration behavior, and the completeness of the link between
experimental results and the precise code and conditions that produced them.

## Appendix A. Validation commands

These commands describe the original check set. Their results are in section 3. Commands
that resolve dependencies used the committed lockfile. Socket-dependent tests ran where
temporary local sockets were permitted.

```bash
cargo test --workspace --locked --offline
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo fmt --all -- --check
bash process/gates/lock.sh
python3 -B process/gates/census.py
python3 -B process/gates/test_census.py
python3 -B process/gates/test_grounds_parity.py
cargo test -p weaver-harness -p weaver-state \
  --features weaver-harness/pyworker,weaver-state/postgres --locked --offline
cargo test -p weaver-web --lib --locked --offline -- --show-output
```

### A.1 Isolated web database and startup

Use a disposable cluster, never an existing database, because the tests apply migrations
and create rows. The review initialized a temporary cluster with `initdb --no-locale
--username=weaver_assessment -A trust`, in a private temporary directory. It started
PostgreSQL with `listen_addresses=''` and a Unix-socket directory inside that same
private directory. No TCP listener was enabled.

The test URL named both the explicit role and the temporary socket directory:

```text
postgresql://weaver_assessment@localhost/postgres?host=<temporary-socket-directory>
```

With `DATABASE_URL` set to that URL, the executed test command was:

```bash
cargo test -p weaver-web --lib --locked --offline -- \
  --test-threads=1 --show-output
```

A temporary server configuration selected that database and set both `listen` and
`link_listen` to `127.0.0.1:0`. The server was then invoked with:

```bash
cargo run -p weaver-web --bin weaver-web --locked --offline -- \
  --config <temporary-config.toml>
```

Expected observation at the assessed baseline: the tests pass and startup exits 1 naming
the absent `participants` relation. Stop the disposable cluster with `pg_ctl -D
<temporary-data-directory> -m fast -w stop` even when a command fails. The review used a
`finally` cleanup path and confirmed that the cluster stopped.

### A.2 Shell deadline reproduction

The following standalone Rust probe was compiled outside the repository against the
workspace's built `weaver_gate` and `weaver_types` debug libraries, using edition 2024
and `-L dependency=target/debug/deps`. It calls the existing public executor without
modifying its implementation. It creates only a short-lived `sleep` process.

```rust
use std::time::Instant;
use weaver_types::{ToolExecution, ToolName};

fn main() {
    let start = Instant::now();
    let result = weaver_gate::tools::execute(&ToolExecution {
        name: ToolName("bash".into()),
        arguments: r#"{"command":"/usr/bin/setsid /bin/sleep 2 & wait"}"#.into(),
        clock_ms: 50,
    });
    println!(
        "Requested clock: 50 ms; elapsed: {} ms; outcome: {:?}",
        start.elapsed().as_millis(),
        result
    );
}
```

Observed output:

```text
Requested clock: 50 ms; elapsed: 2009 ms; outcome: Killed { partial: None }
```

The timing is one observed sample, not a latency distribution. The relevant property is
that the two-second descendant lifetime governed completion despite the 50 ms request.

## Appendix B. HADES snapshot receipt

The queried database was `WeaverTools_v5` and the named graph was `codebase_graph`.
`orient`, `db_collections`, `graph_list`, `graph_neighbors`, `db_query`, and projected
`db_list` reads were used. No graph mutation or ingest was requested.

| Object | Observed count |
|---|---:|
| Indexed documents | 46 |
| Indexed code files | 188 |
| Chunks and embeddings, each | 3,043 |
| Assertions | 418 |
| Code-to-assertion edges | 504 |
| Code symbols | 4,868 |
| Call edges | 3,431 |

The code profile was listed with `path`, `content_hash`, and `ingested_at`. The document
profile was listed with `source_rel`, `content_hash`, and `ingested_at`. SHA-256 hashes
of local file bytes were compared with each returned content hash. The five differing
code paths at the original baseline were:

```text
process/gates/census.py
process/gates/test_census.py
crates/weaver-state/src/main.rs
crates/weaver-state/src/store.rs
crates/weaver-state/src/engine/postgres.rs
```

The eight differing document paths were:

```text
docs/crates/weaver-agents-PRD.md
docs/crates/weaver-harness/Loops/basic-inference-loop.md
docs/crates/weaver-harness/Loops/diagnostic-replay-loop.md
docs/crates/weaver-harness/weaver-harness-Spec.md
docs/crates/weaver-harness/weaver-state/weaver-state-Spec.md
docs/crates/weaver-spu/weaver-spu-Spec.md
docs/crates/weaver-web/weaver-web-PRD.md
docs/crates/weaver-web/weaver-web-Spec.md
```

These counts are receipts for that query and checkout. They are not standing claims
about HADES freshness or the number of assertions in every future version of the corpus.

## Appendix C. Source map

Links below resolve within the repository. Their content can change after this report.
The original evidence is recoverable with `git show c30b4b9:<path>`, and the publication
update with `git show f03142a:<path>`.

[tool-grant]: ../../crates/weaver-harness/src/tools.rs
[diagnostic-gate]: ../../crates/weaver-analysis/src/reading.rs
[reproducibility]: ../technical/weaver-agents/reproducibility.md
[readout]: ../../crates/weaver-spu/src/readout.rs
[web-main]: ../../crates/weaver-web/src/bin/weaver-web.rs
[conversation]: ../../crates/weaver-web/src/store/conversation.rs
[shell-executor]: ../../crates/weaver-gate/src/tools.rs
[harness-execute]: ../../crates/weaver-harness/src/engine.rs
[pyworker-main]: ../../crates/weaver-harness/src/bin/pyworker/main.rs
[pyworker-loop]: ../../crates/weaver-harness/src/bin/pyworker/py_loop.rs
[web-test-helper]: ../../crates/weaver-web/src/store/read.rs
[postgres-engine]: ../../crates/weaver-state/src/engine/postgres.rs
[unfinished]: sketch-what-is-not-built.md
[working-structure]: ../../crates/weaver-trace/src/structure.rs
[trace-writer]: ../../crates/weaver-trace/src/writer.rs
[web-charter]: ../crates/weaver-web/weaver-web-PRD.md
[analysis-parser]: ../../crates/weaver-analysis/src/record.rs
[apex]: ../crates/weaver-agents-PRD.md
[calculator]: ../technical/weaver-agents/weaver-internal/calculator.md
