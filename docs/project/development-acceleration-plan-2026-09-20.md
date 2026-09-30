# WeaverTools development acceleration plan

**Version:** v0.1, 2026-09-20.

This is a draft advisory plan from the independent review seat. It authorizes no
implementation, changes no governing rule, and makes no release claim. The objective
is less elapsed time and operator attention from an agreed requirement to integrated,
verified behavior. Estimates below are planning judgments, not measured throughput.

## 1. Baseline and evidence

### Facts found

The starting point was the [independent assessment][assessment]. Three baselines must
remain distinct:

| Baseline | Commit | Meaning |
|---|---|---|
| Original survey | `c30b4b9aa4ee14d0cd4e958fb1572c46c75b6049` | Most runtime reproductions and the original workspace test run |
| Publication source | `f03142a85e1c6bf088b6299063455272df93d68d` | Four intervening upstream changes inspected before publication |
| Assessment publication and this checkout | `2729350a9ce9d393b72c7ddb7d4da3d38e22712b` | Report and AGENTS.md added, otherwise the publication source |

A fresh fetch found `origin/main` at the checkout commit. The tree was clean before
this draft. GitHub inspection found 38 open issues and one open PR, [#640][p640].
The bounded history sample below was retrieved on 2026-09-20. Open means unresolved
in GitHub, not necessarily unimplemented or actively blocked.

Read authorities: [AGENTS.md][agents], [CLAUDE.md][claude], all four process documents
([Process][process], [Rules][rules], [Format][format], [Handoff][handoff]), and the
[apex][apex]. Relevant charter, contract, Spec, workflow, and source sections were
read for execution, custody, Python composition, state, web startup, and replay.
The reference register below locates the authorities each package must reopen.
Stale position prose does not override current clauses or source.

HADES was available through the existing authenticated connection, read-only. Its
`WeaverTools_v5` snapshot contains 188 code metadata entries and 46 document entries,
with 1,993 code chunks and 1,050 document chunks. Code ingestion timestamps span
2026-09-17 16:11:55-16:13:58 UTC, documents 16:14:34-16:16:29 UTC. All 188 indexed
code files and all 46 indexed documents matched checkout SHA-256 hashes. This is
coverage of the indexed set, not the entire repository, and no aggregate source
commit was returned. The assessment's earlier mismatch counts are historical.

A targeted neighbor query for
`wt_assertions/harness-tool-result-granted-not-minted` retrieved its declaring
harness Spec and citing `src/tools.rs`. Current source confirms that grant boundary.
HADES supplies impact pointers, not authority to bypass the gate or mint results.
The process document's historical statement that HADES is down is not today's
operational observation. Governing process files were read directly.

No new full suite, deployment, live cancellation run, or GPU experiment was conducted
for this plan. The assessment's retained evidence includes 632 reported host passes,
133 optional-feature passes that overlap them, 23 web passes with a disposable
PostgreSQL database, and the failing startup and shell probes. Its publication check
adds 22 state tests with PostgreSQL enabled and 35 census tests. These are dated
results at their stated commits, not fresh certification of this draft. Current
source inspection establishes persistence of the mechanisms, not a new reproduction.
No hardware execution slot or complete device-suite result was established here.

### Findings rechecked

| Finding | Current evidence and disposition | Confidence |
|---|---|---|
| Startup | `weaver-web/src/bin/weaver-web.rs` still reconciles providers and roles after opening the new store. The assessment's fresh-database startup failed on missing `participants`. Composition-root repair remains open. | High |
| Shell deadline | `weaver-gate/src/tools.rs` still kills a process group and joins pipe readers without a separate bounded drain. The recorded 50 ms request took 2,009 ms with a detached descendant. No-orphan and bounded-return acceptance remains unmet for that case. | High |
| Stop during execution | Harness `engine.rs::execute_call` blocks on gate reception rather than servicing the coordination listener. End-to-end stop latency is unmeasured. The gate Spec also records deferred cancellation. A deadline repair alone cannot certify stop. | High on mechanism, medium on operational extent |
| Python provenance | The worker hashes once before `serve`, then rereads and compiles the file per crossing. Trace Spec section 3 explicitly limits the digest to load time. This is a declared evidence limitation, not a newly discovered violation of that clause. | High |
| Validation reporting | Database helpers can return successfully without a database, while optional targets may compile zero tests. #589 names eight read tests, the broader assessment found 19 conditional web tests. Neither number measures current coverage by itself. | High |
| PostgreSQL parity | #638 fixed index naming and refusal. Its PR also records live-server checks, including escaping with `standard_conforming_strings` off. What remains is a repeatable live-engine parity lane, not an unfixed collision or absence of all server evidence. | High |
| Web evidence ingestion | #538 remains open, and `analysis/src/main.rs` explicitly names the missing summary fields. `GenerationSummary` still carries five members. Starting the server will not make its run ingestion complete. | High |
| Long-running operation | Trace working storage accumulates events, while the writer queue blocks at capacity. This is a retention/backpressure election, not proof of a leak. No suitable soak evidence was established. | High on mechanism, low on sustainable duration |
| Lock hygiene | #551 remains open, but its latest comment confirms all four implementation asks landed. `lock.sh` and locked commands exist. Only the rule-placement decision remains. Do not schedule the fixes again. | High |

## 2. Where convergence is losing time

### Bounded history sample

Six recently merged changes were selected to include behavior, tests, documentation,
and process. One open PR and one withdrawn PR expose different unfinished outcomes.
This is a purposive sample, not an estimate of all development effort. Review-seat
reports appear both as comments and formal reviews, so API review counts are not
review rounds. PR bodies can be edited after events and are read with dated comments.

| Change | Observable sequence and what the work resolved | Open-to-merge interval |
|---|---|---|
| [#638][p638], PostgreSQL naming | First seat pass found 13 issues, including an unobserved emitted-name property and a swallowed refusal. Rework and a second pass held those properties. Final review found string escaping, answered with live-server evidence. | 158 minutes |
| [#636][p636], census scope | Review showed combined perturbations concealed unwatched CUDA/TOML paths. Rework separated fixtures and checks. Later review caught citations inside Python strings. Rebased after #634's shared Format edit. | 204 minutes |
| [#634][p634], charter context | Clarified what outside-domain charters belong to and avoided turning the missing domain graph level into a settled rule. Author answered 14 findings. A review measurement itself needed correction. | 90 minutes |
| [#630][p630], replay claim ownership | Restated claims at the responsible crate, corrected a misapplied shared-claim rule and stale graph counts. Multiple seat passes, a rebase, and follow-up wording/debt corrections are visible. | 132 minutes |
| [#597][p597], lock gate | Moved checking before Cargo can repair the subject. Review distinguished drift, unavailable offline resolution, and git errors. Second pass exercised each arm. | 172 minutes |
| [#616][p616], trace watch | Proposed retirement of a weak test also removed a real pairing watch. Review demonstrated a mutation passing the crate suite, rework restored the useful watch, second pass verified deletion and widening mutations. | 252 minutes |
| [#640][p640], open | Test-only RoPE watch is still one commit. Seat findings distinguish a copied formula from exercising the fork constructor, and identify missing sine coverage. No disposition of those findings was visible. | Not merged |
| [#622][p622], withdrawn | Review found the technical-page act was already presumed out of scope. Operator withdrawal closed it without a merge. The dependency summary had omitted that scope condition. | Not merged |

Intervals are rounded wall-clock PR-open to merge times, not hands-on implementation
or review durations. Much implementation precedes PR creation. The histories expose
validation, rework, rebasing, and operator dispositions but do not allocate every
minute between them. Gaps have no assigned cause. CodeRabbit success is the visible
check context in the six merged samples, not proof of an automated workspace suite.

Open-work distinctions matter. [#538][i538] is a concrete producer/consumer dependency.
[#594][i594] awaits a registered external service and does not block bash.
[#495][i495] explicitly says "Not queued", requiring batch machinery before its
experiment, so its age is not delay. [#639][i639] stages dependency work behind a
credible watch. #640's inactivity does not establish hardware or operator waiting.
[#551][i551] is a remaining operator question after implementation. [#589][i589]
is an unresolved reporting defect, without evidence assigning a reason for its age.

### Advice: ranked bottlenecks

| Rank | Likely bottleneck | Evidence, milestone impact, and recurrence | Confidence |
|---|---|---|---|
| 1 | Acceptance arrives after component success | Web tests pass while startup fails. Shell unit success did not cover detached children. #538 leaves a consumer unable to ingest its promised record. Directly limits usable behavior. | High that gaps exist, medium that this dominates total lead time |
| 2 | Instruments and claims need repeated repair | #638, #636, #616, #597 and open #640 contain measured holes in what checks observe. Reviews buy correctness, but earlier targeted checks could prevent some rounds. | High recurrence, unmeasured effort share |
| 3 | Reconstruction of scope, status, and shared edits | #622's withdrawn scope, #551's landed asks, #630's rebase/count repair, and #636/#634's Format collision are concrete instances. | Medium-high |
| 4 | Environment-dependent evidence is ambiguous | Conditional database/model tests, zero-test Python target, and a previously timed-out CUDA build. Adds uncertainty before integration and can consume operator setup time. | High uncertainty, unmeasured setup burden |
| 5 | Implementation concurrency may be underused | Apex 5.3 licenses independent contract implementations. Separate startup and execution surfaces exist, but shared harness edits and review capacity constrain useful overlap. | Medium opportunity, low evidence of spare capacity |

Do not begin with a comprehensive process audit. Repair the known deadline and
startup cases while collecting outcome-level timing. There is no evidence here for a
percentage speedup, a staffing shortage, or a conclusion that documentation work is
waste. The sampled documents resolved authority, custody, interface completeness,
and misleading enforcement claims.

## 3. Selected milestone: one repeatable serving-to-replay workflow

### Recommendation and supported configuration

Select **M1: a clean installation completes and records a real tool-using session,
replays it through the diagnostic binding, and compares one declared variation**.
This advances apex sections 1, 4 and 8 without making a new application the deliverable.
Use existing admin, gate, compiled worker, and analysis consumers. Web startup is a
parallel repair, not the acceptance driver. Its unfinished ingestion and surfaces
would otherwise turn the milestone into the wider web program.

The proposed release profile is Linux with systemd, operator-provisioned identities,
the pinned Rust toolchain, one native CUDA Qwen2.5-0.5B-Instruct residency, compiled
Rust loop, SQLite state member, and an operator-owned file sink. The native fixture
already has `WEAVER_ARTIFACT_QWEN25_SAFETENSORS` and the documented fallback directory
`/bulk-store/models/Qwen--Qwen2.5-0.5B-Instruct`. Pin the actual artifact, tokenizer,
configuration, binary and dependency digests before the first acceptance run.

Olympus is the proposed deployment/device lane, subject to an available slot and
preflight. This plan does not assert that it is provisioned or idle. Host development
also uses the existing CPU GGUF fixture
`/opt/weaver/models/qwen2.5-0.5b-instruct-q6_k.gguf`. CPU results do not certify the
native device profile. Build the selected stack with
`cargo build --workspace --features weaver-spu/cuda --locked`, recording the resolved
features and profile. A single-device milestone does not replace the Process's
complete device suite, which needs its declared pair and fixtures.

Operating envelope: one serving agent, serialized turns, at most 20 turns per
session, at most 30 minutes, healthy local file sink, no multi-agent throughput or
unattended continuous-service claim. Propose a 512 MiB combined peak RSS budget for
worker, gate and state, excluding the SPU's model residency. Record SPU host/device
peaks separately against admission capacity. These are proposed acceptance limits,
not existing product promises. A failed budget is investigated, not silently raised.

### Observable completion conditions

| Check | Required evidence |
|---|---|
| A1. Clean environment | An isolated supported host starts from an empty application database/state directory, no running agents, and a clean checkout. Provisioning prerequisites are explicit. Build and installation identify one commit and all executable hashes. A second clean run needs no undocumented operator repair. |
| A2. Live serving | Admin validates and loads. An authorized client sends through Gate. The real model answers two consecutive turns, with resident cache continuity and a harness-directed flush demonstrated separately. No fake model substitutes for this run. |
| A3. Real tool path | A model-emitted bash call crosses harness execution to Gate under the agent identity, returns its four-way outcome through the granted-result path, and the model consumes the result. Predeclare a harmless `printf` task, prompt and a maximum of five attempts. Preserve unsuccessful attempts. A synthetic parser fixture remains a separate seam test. |
| A4. Custody and trace | Operator inspects load/unload and turn brackets, causal keys, canonical payloads, exact tool result and measurement attribution. Negative checks show the agent cannot open the sink, read it through process handles, or inherit it into the shell. Unload drains under the declared healthy-sink condition. |
| A5. Bounded execution | A 50 ms detached-child case answers within 250 ms on the reserved unloaded host and leaves no invocation descendant alive. Test ordinary children, `setsid`, inherited pipes, excessive output, refusal and machinery failure. Record timing distribution and scheduler conditions. The 250 ms tolerance is a proposed test bar, not permission for a second kill clock. |
| A6. Activity control | Stop during decode and tool execution returns to loaded-and-idle, closes the turn once, and leaves no orphan or late answer assigned to a later turn. Target acknowledgement within one second under healthy sink conditions. A subsequent turn succeeds without reloading. Ratify missing cancellation semantics before claiming this case. |
| A7. Replay | Outside-agent analysis derives the diagnostic declaration, admin loads it, analysis preloads and seals, and the existing reader reports certified. No Gate is raised for the diagnostic run. Tokens are re-fed from the record, not re-sampled. Missing required provenance, a changed token and an unsealed/truncated record cannot become a valid certificate. |
| A8. Declared variation | Before running, register one change: residual readout off versus on at reload, same weights, binary, seed, prompts, sampling, device and state election. Compare serving token/measurement series with the existing signals reader. Report equal, different or incomparable with the changed condition and trace digests. No claim that a fixed seed guarantees repeatability or that a difference proves benefit. |
| A9. Bounded endurance | Complete the declared 20-turn session within its envelope, measure memory and queue pressure, stop/unload, then repeat from empty state. A finite slow-sink pause resumes without silent loss. Permanent sink stall is separately reported as unsupported availability, not hidden as a successful cancellation test. |

For A7, use `weaver-analysis derive`, `preload`, and `read` in the ordering of the
[diagnostic loop][replay-loop]. Use `compare` only for compatible certified capture
artifacts. It is not a generic comparator for different stochastic serving runs.
A8 can use a small acceptance-side comparison of the existing signals output and
canonical records, without adding consumer reasoning to the primitive.

### Reconcile the apex definition of done

A1-A6 and A8 exercise items 1-5 and 7 of apex section 4, subject to passing evidence.
**M1 does not claim the full apex deliverable. Item 6 remains separate.** A bash
calculation and the pure calculator's unit tests do not demonstrate the required
harness-supplied deterministic replacement.

[Internal PRD][internal] section 3 leaves the calling surface pending. Section 5
names the signal-exists, actionable-signal, and beats-deliberate-loop ladder for
autonomic wiring. Apex item 6 describes model-elected protoautonomic mechanics.
The architecture seat must reconcile the elected demonstration with that calling
surface and the splice work, without demanding efficacy evidence of a primitive or
bypassing gates on a stronger autonomic claim. Commission that bounded authoring
question in parallel. If the operator selects full apex completion instead of M1,
its ratified outcome and calculator integration join the critical path. No estimate
for that unshaped implementation is offered.

## 4. Execution structure and handoffs

The integration owner is a role assigned to one implementation seat for M1. That
seat owns the supported profile, combined branch state, acceptance driver and final
evidence. It does not acquire authority over another domain's elections. One
independent review seat must be available before adding a second implementation
lane. Roles below are not claims about additional people or available sessions.

Start with one active implementation lane and the integration work it can sustain.
Allow at most two implementation lanes when their reviewer and test slots are
reserved. Startup can overlap execution. Python work touching the harness waits for
an agreed file split or execution's merge. Stop starting new work if the reviewer
queue exceeds one ready outcome. Review throughput, not agent count, limits expansion.

### Dependency and collision map

| Work | Depends on | Parallelism and shared surfaces | Resource constraint |
|---|---|---|---|
| W0 acceptance skeleton and profile | None | Feeds all lanes, integration owner holds scripts and result format | Host plus later deployment slot |
| W1 bounded shell execution | W0 failing case, existing execution clauses | Parallel with W2, owns Gate tools and execution tests | Linux process/identity tests |
| W1c stop semantics and integration | W0 characterization, narrow authoring ruling if needed, W1 | Shares harness engine and gate channel with Python or protocol edits | Integration review, privileged lifecycle lane |
| W2 clean web startup | W0 startup case | Parallel with W1, owns web composition root and startup test | Disposable PostgreSQL |
| W3 Python provenance | Narrow authoring decision first | Host work parallel, serialize shared harness/trace edits with W1c | Python development headers/runtime |
| W4 assembled M1 acceptance | W0, W1, W1c and selected profile available | Critical-path fan-in, compiled loop avoids W3 dependency | Reserved native GPU and systemd host |
| W5 live state parity | W0 database profile | Parallel, owns state engine tests, avoid #638 follow-up conflicts | Disposable PostgreSQL and SQLite |
| W6 analysis/web run ingest | #538 contract obligations | Parallel later, conflicts with W2 only at web ingest/startup glue | PostgreSQL, valid deposit fixtures |
| W7 calculator commission | Apex/internal seam reconciliation | Authoring parallel, eventual harness/SPU work serialized | Operator ruling, later device evidence |

Critical path: W0 -> W1 -> W1c -> W4, with an authoring branch feeding W1c and a
hardware reservation feeding W4. W2 is valuable but outside the selected CLI
milestone. W5 becomes a prerequisite only for a PostgreSQL state deployment. W6
becomes one only if web-held run comparison is added. No floor or wire revision is
shared across concurrent implementations before its governing act settles.

Reserve `crates/weaver-harness/src/engine.rs` to W1c while it is active. W3 owns
`src/bin/pyworker/` but coordinates any shared `LoopIdentity`, trace payload or
reader edits before dispatch. W2 and W6 serialize changes to web startup and ingest.
The integration owner alone updates shared acceptance scripts and any batch baseline.
Keep W4's device slot separate from #640/#639's SPU validation runs so cache and
resource contention do not become unexplained evidence differences.

### Compact task manifests

Every implementation handoff carries base
`2729350a9ce9d393b72c7ddb7d4da3d38e22712b` initially. At dispatch, replace it with the
exact integrated parent and name intervening commits. Each row below supplies the
behavior, consumers, scope, environment and acceptance. Attach only relevant clause
links and the current failing evidence, not copies of governing text. The actual
handoff follows [Handoff sections 2-4][handoff], including reached documents, owed
changes, exclusions, gates, and questions for the receiving seat.

Estimates are focused implementation-session hours including local tests and one
rework allowance. They exclude operator availability, review queue, cold builds,
model downloads and hardware waiting. Review hours are separate reviewer effort,
not elapsed-time promises. Work can exceed the range if a new contract is required.

| Package and owner role | Outcome, clauses and affected consumers | Completion evidence, environment, exclusions | Estimate and decision |
|---|---|---|---|
| W0, integration/acceptance | Repeatable profiles, failing assembled smoke cases and one result manifest. Process 6, apex 11, current lock/census scripts. All implementation lanes consume the output. | Reproduce startup and deadline failures, absent required DB/model produces visible non-pass. Linux host and isolated DB. Excludes new CI platform or gate policy. | 4-8 hours plus 1-2 review. Existing commands and reproducer material. Operator selects profile and owner, no review-rule change. |
| W1, execution lane | Bound invocation lifetime and output drain without losing outcome taxonomy, identity or custody. Gate Spec 8, harness-gate contract 2. Gate executor and harness tool consumer. | A3/A5 and mutation checks, no surviving detached descendant, existing success/refusal behavior retained. Linux. Excludes new tool registry and arbitrary containment redesign. | 8-20 hours plus 2-4 review. Cross-process supervision is the uncertainty. If current process-group election cannot enforce the promise, commission the narrow Spec change before that implementation. |
| W1c, integration/execution | Make activity stop coherent during a live tool exchange. Apex 6, harness Spec 6.1-6.2, admin-harness and harness-gate contracts, Gate Spec 7. | A6 through admin, gate, tool and trace, with reply-race and subsequent-turn tests. Privileged isolated Linux lane. Excludes cancellation invented solely in a consumer. | 6-16 hours plus 2-4 review, after a 2-4 hour authoring characterization. Operator ratifies any missing exchange and representation semantics. |
| W2, startup lane | Start the current web instrument against its current schema. Web Spec 2, 6-8. Server, migrations, configured connector boundary. | Fresh DB startup, meaningful existing surface read, restart, absent DB failure, connector disconnect reported. PostgreSQL. Excludes restoring retired chat schema, IAM, new surfaces and complete run ingest. | 4-10 hours plus 1-3 review. A known composition mismatch, remainder bounded by existing routes. Escalate a missing product decision, not routine wiring. |
| W3, provenance lane | Make Python evidence identify executed bytes, including exceptions/fallback. Trace Spec 3 and basic loop/harness grants. Pyworker, trace and analysis readers. | Edit-between-turns and read/hash/execute race tests. Record matches the executed source, failure and fallback remain attributable, old records remain readable. Python host. Excludes production certification until complete. | 2-4 hours authoring, then 6-14 implementation plus 2-4 review. Decision: preserve hot reload with per-crossing identity, or create immutable run boundaries. Recommend the former for iteration, after ratification. |
| W4, integration owner | Execute A1-A9 through existing consumers and publish a reproducible evidence bundle. Apex 4/8, custody contracts, replay loop, analysis Spec 2-5, SPU admit/decode clauses. | Two clean runs at the final integrated commit, healthy and refusal cases, all conditions and unavailable lanes stated. Native single-device Linux profile. Excludes efficacy claims, UI development and full apex item 6. | 8-16 hours plus 2-4 review. Existing components, substantial orchestration and custody checks. Requires reserved deployment/device lane and envelope agreement. |
| W5, database lane | Repeatable common store semantics on SQLite and live PostgreSQL. State Spec 3-4 and harness-state/analysis-state contracts. Serving and diagnostic state consumers. | Same fixture checks selection, order, session isolation, refusal and index catalog behavior, including escaped keys and server identifier width. Disposable databases. Excludes general SQL backend expansion. | 5-12 hours plus 1-3 review. Existing engine tests are starting points. No milestone block for SQLite. |
| W6, consumer integration lane | Complete #538's emitted summary through ingest to one web run read. Analysis-web contract 2.2, analysis Spec 5/7, web Spec 3.1/4. | Real record/deposit -> emitter -> DB -> read, absent required condition refused without fabricated default. Excludes queue runner, Models and general web buildout. | 10-24 hours plus 2-4 review. Deposit reader is explicitly owed, so this is more than adding fields. Not scheduled in wave one. |
| W7, architecture seat | Produce a ratifiable calculator integration commission that distinguishes elected mechanics from autonomic efficacy. Apex 4/9, internal PRD 3/5 and Spec 2. | Defined caller, result attribution, grant compatibility, governing changes and testable item-6 demonstration. No implementation. | 3-6 hours plus operator decision time unestimated. Later implementation estimated only after the commission settles. |

Do not sum these into a calendar promise. W0/W1/W1c/W4 contain 26-60 focused
implementation hours plus the W1c authoring allowance and 7-14 reviewer hours.
Overlap can reduce elapsed time only if independent capacity and hardware exist.
Moving verification to another machine or reviewer does not remove that work.

## 5. Repeatable validation and evidence

Reuse `process/gates/lock.sh`, `census.py`, the census/parity tests, Cargo, existing
fixtures, and deployment scripts. Add thin profile orchestration rather than a new
validation framework. Resolve dependencies under `--locked`, warm a cold cache in a
separate recorded preparation step, and preserve command exit codes and stderr.
A full-resolution offline lock check returning 2 is unavailable, not drift or pass.

| Profile | Required setup and representative checks | Claim it supports |
|---|---|---|
| Host | Pinned nightly, C/C++ and build dependencies. `bash process/gates/lock.sh`, `cargo fmt --all -- --check`, changed-crate clippy with `--all-targets --locked -- -D warnings`, `cargo test --workspace --locked`, `python3 process/gates/census.py`, gate fixture tests. Record resolved features. | Host compilation and exercised tests, not external resources silently bypassed |
| Database | Disposable PostgreSQL with explicit URL and migrations. Web library/startup tests, plus `cargo test -p weaver-state --features postgres --locked` and W5 live cases. Separate SQLite results. | Actual database behavior, not feature compilation alone |
| Python | Declared Python interpreter/ABI and development libraries, `cargo test -p weaver-harness --features pyworker --locked` plus new crossing/fallback tests. | Execution provenance, zero tests cannot certify it |
| CPU inference | Exact GGUF artifact/tokenizer identities, enough RAM, explicit fixture paths. Loaded-model and serving smoke checks. | CPU GGUF behavior only |
| Device | Exact CUDA, driver, GPU model/ordinals, fork pins and artifacts. Single-device M1 checks, separately `cargo test --workspace --all-features --locked` on the required pair with all fixtures. | Selected device coverage, full-suite green only when all required device cases execute |
| Deployment/acceptance | Provisioned identities, systemd, declared directories and sink custody, matching binaries. A1-A9 and disposable web startup as separately selected. | Whole installed workflow at one commit |

Each profile declares required cases before execution. Record each case as `passed`,
`failed`, `skipped`, `unavailable`, or `not selected`. Passed means assertions ran.
Skipped means a selected case intentionally did not execute and carries a reason.
Unavailable means setup prevented execution. Not selected means outside the declared
profile. Missing required setup makes the profile exit nonzero and remain uncertified,
while preserving the distinction between infrastructure absence and product failure.
Rust tests that return early require explicit execution evidence or a fail-fast
fixture mode before their result can count as passed. Parsing a green total is
insufficient.

The evidence bundle records commit and dirty diff, commands and exit codes, timestamps,
resolved features, compiler/build profile, lock and executable hashes, fixture/model
and tokenizer hashes, OS/kernel, Python ABI, database version/settings, CUDA/driver,
device model/ordinals, effective sampling and seed, state election, trace/capture
hashes, and requested versus executed cases. Use current canonical trace fields and
an operator-side run manifest for test conditions. Do not add reserved product fields
merely to feed CI. Keep experimental deposits in the existing experiments repository
under its dated-record discipline, with references rather than mutable copies here.

Before replay acceptance, an external validator accounts for every source line and
required event. The analysis parser's permissive malformed-line behavior is not
silently changed. Negative fixtures must prove that the assembled acceptance checker
refuses to certify insufficient evidence. A parser policy change is a separate
ratification request.

Retain H1-H6 and applicable G gates. Run deterministic checks before the first review,
answer each finding, rerun affected gates and seek review after substantive rework.
Changes in final review also receive the required rework review. Preserve draft PRs,
CodeRabbit's final position and existing round limits. After independent merges, run
the assembled checks on their combined commit. Branch results alone do not certify
integration. Regenerate a shared census baseline once after the batch settles, under
the existing no-new-defect rule, never to absorb a regression.

## 6. First execution wave

These are the first five future tasks, in order. This plan performs none of them.

1. **Commission W0 and reserve the integration checkpoint.** Record the selected
   profile, exact base, acceptance cases, integration owner, reviewer and proposed
   hardware slot. Availability is explicit. No broad issue audit is needed.
2. **Turn existing failures into repeatable acceptance observations.** Run the
   empty-DB startup and detached-child deadline cases, characterize stop during a
   tool call, and make missing prerequisites visible. Save one baseline bundle.
3. **Dispatch W1 with its narrow authoring dependency.** Repair the known deadline
   failure against the one-clock/no-orphan obligation. In parallel, the architecture
   seat settles the stop semantics required by W1c. Continue unaffected executor work.
4. **Dispatch W2 only when a second lane and reviewer are available.** Complete
   startup through an existing web surface. Otherwise serialize after W1. Prepare
   W4's operator-side driver without changing agent semantics.
5. **Integrate W1/W1c and run W4's first full checkpoint.** Use one reviewed combined
   commit and the selected device host. The checkpoint must reach live tool result,
   custodied trace, certified replay and declared variation, and report failures
   through cleanup. Retest after fixes, then perform the second clean acceptance run.

Checkpoint success is a complete evidence bundle with A1-A9 passed and no required
case skipped or unavailable. Checkpoint failure produces a bounded next task at the
failed seam, not another broad implementation wave. W2 has its own acceptance result
and does not silently become part of M1's critical path.

Deliberately defer W3 implementation pending its provenance ruling, W5 until a service
state profile is wanted, W6 until web run ingestion is selected, and calculator code
until W7 settles. Also defer general application surfaces, queue/batch machinery,
external service registration, memory, family expansion and dependency migration.
Keep #640/#639's correctness work visible, but do not repin Candle to accelerate M1.
Do not make domain-node notation, IAM redesign, infinite-session retention or an apex
architecture debate prerequisites for a bounded trusted-host demonstration.

## 7. Acceleration investments and their costs

### Permitted by current rules

| Proposal | Recurring cost removed | Setup and maintenance | Measure of value |
|---|---|---|---|
| Thin acceptance profiles, W0/W4 | Recreating environments and discovering assembly failures late | W0/W4 estimates above, update when selected dependencies or commands change | Fewer manual setup interventions and earlier detection, including cold-start latency |
| One compact handoff per outcome | Repeated discovery of base, authority, exclusions and affected consumers | About 30-60 minutes to prepare initially per package, refresh on rebase or scope change | Time to first valid reproduction and fewer scope corrections at review |
| Targeted impact packet | Repeated whole-corpus searches and missed consumer checks | About 1-2 hours to standardize source hashes and links, 5-15 minutes per changed seam | Context-preparation effort, downstream defects, stale-pointer incidence |
| Pre-review perturbation receipt | Review spent proving that a claimed watch watches something else | Record mutation applied, failing named test, restored-tree pass. Usually 0.5-2 hours per new behavioral property, potentially more for devices | Review findings about false coverage, balanced against added author time |
| Read-only status summary | Manually rediscovering landed asks and selecting withdrawn work | 2-4 hours for a small PR/issue/commit query, maintain queries only. Link authoritative rulings and mark unresolved scope, do not copy them as new authority | Duplicate starts and status-reconciliation interventions |
| Limited concurrency with integration ownership | Idle time on independent edits without multiplying collisions | Scheduling/checkpoint work about 0.5-1 hour per wave, plus normal integration | End-to-end outcome time, reviewer queue and rebase/rework effort |

These costs are assumptions to time, not savings already realized. HADES freshness
checks must compare relevant source hashes. A missing graph node requires current-file
inspection, not a conclusion of no impact. The status summary is disposable derived
output, not another governing roster. Authoritative prose remains at its source.

### Requests to reopen or ratify decisions

| Decision | Recommendation and risk |
|---|---|
| M1 versus full apex completion | Select M1 as a bounded intermediate. Risk: calling it done could conceal item 6, so report the apex item mapping with every milestone result. |
| Stop and escaped-descendant mechanics | Settle only missing contract/Spec semantics that W1/W1c expose. Preserve one caller clock, result grant, and identity custody. Risk: a local timeout workaround can return while work remains alive. |
| Python crossing identity | Preserve hot reload with exact executed-byte attribution if iteration is wanted. This changes the explicitly limited provenance promise and reaches trace/readers. Risk: schema work grows, or failures falsely name the requested script as executed. |
| Calculator commission | Clarify the elected item-6 demonstration, pending calling surface and autonomic ladder. No bypass of granted results and no efficacy claim from a pure function test. |
| Formal approval or review rules | No change proposed for M1. Promoting profiles into mandatory gates, replacing status registers, or changing review/ratification authority would require a later explicit ruling with evidence. Faster merges alone would not justify weaker verification. |

The assessment's broader reopen requests remain advice, not prerequisites. Current
lock hygiene can be used without resolving #551's remaining rule-placement question.

## 8. Lightweight measurement alongside development

Track one record per agreed observable outcome, not per PR. Start with W1 and W2,
then W4. Use the baseline bundle and PR references already produced. A small append-only
operator-side table is sufficient. Do not build a process analytics product.

| Measure | Definition and guard against misleading improvement |
|---|---|
| Outcome lead time | From recorded agreement on behavior and acceptance to the final combined commit passing required integration checks. Include rework and waiting. Splitting a PR does not reset the outcome clock. Record scope additions separately. |
| Review rounds | Substantive finding -> author disposition/rework -> verification cycles across all PRs in the outcome. Count reports in comments too. Distinguish required rework passes from clean passes. |
| Validation latency | Request-to-start queue, command runtime, and result-to-usable-evidence time separately. Label cold/warm cache and hardware waits. Unavailable runs and retries remain in the record. |
| Operator interventions | Count and briefly time setup repair, requirement decision, permission/provisioning act, and manual evidence reconciliation. Distinguish necessary ratification from avoidable repair. Do not count every message as an intervention. |
| Post-integration defects | Failed accepted condition or regression found after integration, attributed to the outcome over its next ten uses or seven days, whichever is later. Report severity, reopen time and incomplete observation windows. |
| Hands-on effort and waiting | Coarse session intervals for implementation, validation supervision, review and rework. Explicit wait categories only when observed. Unknown remains unknown. Report compute and reviewer effort separately. |

After the first two outcomes and M1, compare similarly scoped work with the sampled
history, acknowledging that the old history lacks effort breakdowns. Publish counts,
ranges and individual outcome times for this small sample, not a misleading percentile.
Report acceptance coverage and escaped defects beside speed. An earlier merge with
verification deferred is unfinished work, not improved lead time. A task cancelled by
an operator ruling is a disposition, not delivered behavior.

Stop maintaining an acceleration artifact whose preparation repeatedly costs more
attention than it saves. Add a second implementation lane only if review queue,
conflict effort and integrated lead time improve without shrinking acceptance.

## 9. Remaining uncertainty and minimum decisions

Operator decisions needed to begin are limited to selecting M1's scope and supported
profile, assigning the integration/review roles, and reserving an isolated device and
provisioning lane. The proposed duration, memory and latency bars should be accepted
or replaced before their runs, never after results are seen. Hardware availability and
required identity privileges remain unverified.

W1c's missing semantics, W3's provenance election, and W7's calculator commission need
their own bounded decisions when their dependent work is commissioned. They do not
hold up W0, startup reproduction or existing-obligation deadline tests. Whether the
small native model reliably emits the requested shell call, whether complete replay
works on the selected installed stack, and whether the chosen envelope meets its
memory bar remain experimental questions. Failures count and guide the next task.

No time study establishes that integration is the largest elapsed-time category.
There is enough evidence to fix its known failures without waiting for that study.

Begin with W0's honest acceptance baseline and W1's deadline repair, with W2 parallel
only when review capacity exists. This should shorten convergence by exposing whole
workflow failures before another implementation batch accumulates behind them. The
expectation is falsified if, across the first two integrated outcomes and M1, these
checks find no earlier actionable defect, operator setup effort does not fall, and
validation or review queues consume the time saved. In that case reduce orchestration
and concurrency before weakening any architectural or evidence guarantee.

## Source register

Local links resolve against the examined commit unless this draft is read on a later
checkout. GitHub histories are observations retrieved on 2026-09-20.

[assessment]: assessment-2026-09-19-project-state.md
[agents]: ../../AGENTS.md
[claude]: ../../CLAUDE.md
[process]: ../../process/WeaverTools-Working-Process.md
[rules]: ../../process/WeaverTools-Working-Rules.md
[format]: ../../process/WeaverTools-Document-Format.md
[handoff]: ../../process/WeaverTools-Handoff-Format.md
[apex]: ../crates/weaver-agents-PRD.md
[internal]: ../crates/weaver-internal/weaver-internal-PRD.md
[replay-loop]: ../crates/weaver-harness/Loops/diagnostic-replay-loop.md
[p638]: https://github.com/toddwbucy/WeaverTools/pull/638
[p636]: https://github.com/toddwbucy/WeaverTools/pull/636
[p634]: https://github.com/toddwbucy/WeaverTools/pull/634
[p630]: https://github.com/toddwbucy/WeaverTools/pull/630
[p597]: https://github.com/toddwbucy/WeaverTools/pull/597
[p616]: https://github.com/toddwbucy/WeaverTools/pull/616
[p640]: https://github.com/toddwbucy/WeaverTools/pull/640
[p622]: https://github.com/toddwbucy/WeaverTools/pull/622
[i538]: https://github.com/toddwbucy/WeaverTools/issues/538
[i594]: https://github.com/toddwbucy/WeaverTools/issues/594
[i495]: https://github.com/toddwbucy/WeaverTools/issues/495
[i639]: https://github.com/toddwbucy/WeaverTools/issues/639
[i551]: https://github.com/toddwbucy/WeaverTools/issues/551
[i589]: https://github.com/toddwbucy/WeaverTools/issues/589

| Work | Authoritative current references |
|---|---|
| Execution | [Gate Spec 7-8](../crates/weaver-gate/weaver-gate-Spec.md), [harness-gate contract 2](../crates/contracts/weaver-harness-gate-contract.md), [harness Spec 6](../crates/weaver-harness/weaver-harness-Spec.md), [admin-harness contract](../crates/contracts/weaver-admin-harness-contract.md), [executor](../../crates/weaver-gate/src/tools.rs), [engine](../../crates/weaver-harness/src/engine.rs) |
| Custody and provenance | [operator contract 3](../crates/contracts/weaver-admin-operator-contract.md), [harness-trace contract 2-4](../crates/contracts/weaver-harness-trace-contract.md), [Trace Spec 3-7 and 11](../crates/weaver-harness/weaver-trace/weaver-trace-Spec.md), [pyworker](../../crates/weaver-harness/src/bin/pyworker/main.rs), [crossing](../../crates/weaver-harness/src/bin/pyworker/py_loop.rs) |
| Startup and ingest | Web Spec 2, 3.1, 4, 6-8 (`git show 112bc65:docs/crates/weaver-web/weaver-web-Spec.md`), [analysis-web contract](../crates/contracts/weaver-analysis-web-contract.md), web startup (`git show 112bc65:crates/weaver-web/src/bin/weaver-web.rs`), [signals](../../crates/weaver-analysis/src/signals.rs) |
| State and replay | [State Spec 3-4](../crates/weaver-harness/weaver-state/weaver-state-Spec.md), [harness-state contract](../crates/contracts/weaver-harness-state-contract.md), [analysis-state contract](../crates/contracts/weaver-analysis-state-contract.md), [Analysis Spec 2-5 and 7](../crates/weaver-analysis/weaver-analysis-Spec.md), [SPU Spec](../crates/weaver-spu/weaver-spu-Spec.md), [decode contract](../crates/contracts/weaver-harness-spu-decode-contract.md) |
