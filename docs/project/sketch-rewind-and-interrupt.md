# Sketch: rewind, retry, and the researcher's interrupt

**Status:** LIVING, outside the set. A sketch in `docs/project/`: it maps into no
graph and nothing is written against it. What it settles moves into the apex, the
contracts and the Specs by acts of their own, and the apex moves only on the
operator's ruling.

**Date filed:** 2026-09-29

**Landing PR:** #750

---

## 0. What this document is

The design for stopping a running agent and continuing from any closed turn boundary of
its record, and for the researcher's interrupt as the first caller of that door. It
rests on one property the record already has: the trace determines what the agent was
presented at every position. `model.request` carries each turn's rendered contribution,
and the full context is "the accumulation of the recorded contributions under their
recorded template identities, from the identity prefix the run's opening records", per
`weaver-trace-PRD` section 3.2, with every recorded flush and elision replayed as
section 3.1 requires. **The accumulation starts from what the run's opening seated**:
the identity, and under a restoring load the conversation the load restored beside it,
which `weaver-harness-Spec` section 6.1 makes prefix material, permanent for the
residency and the floor a flush returns to. So a replayed flush returns to that floor
rather than to the identity alone. So a state the agent stood in can be stood up again
from the record rather than reassembled from ingredients, and an exact rewind rests on
the record together with the deployment tuple. It is written on the operator's word of
2026-09-29, in his framing: the trace is the append-only source of truth, a reload is a
bookkeeping event in it, and the power of a trace this detailed is the ability to rewind
and try again from a given spot.

## 1. The door: preload to a position

The corpus has a preload door, the restore election of the operator's ruling of
2026-09-04 on #432, per `weaver-state-PRD` section 4 and
`weaver-analysis-state-contract`, which stands a session up from its record, whole or
cut at a named turn. A restored conversation is recorded under `message.restored`, per
`weaver-trace-Spec` section 3, and a diagnostic binding's record opens with
`replay.opened`, per `weaver-diagnostic-Spec` section 4. This sketch adds one parameter
to it, the position, and one rule of lineage.

**A continuation from position N of run R is a new run under the same session, and its
opening event names R and N.** N is a closed turn boundary, the cut
`weaver-analysis-Spec` section 4's `--through <run>:<turn>` already takes, through that
turn's close, and never a position inside a turn, since a cut inside one would land a
generation without its close. Nothing in R is rewritten and nothing is removed. The new
run's first event after its load is the branch event. It carries the parent run, the
branch position, the reason (a retry, an interrupt's resumption, a diagnostic entry),
the world's disposition, one of section 2's three (reset to N, divergence measured with
what was found, or a changed world declared), and what the record holds of the
deployment tuple the branch stands on: the `load` payload's `stack` digests and
declaration digest, per `weaver-trace-Spec` section 3, and a `weights_hash` named as an
expectation, since the branch event precedes any measurement of its own. The expectation
is the nearest ancestor's `weights_hash` along the lineage. Where no ancestor has one, a
parent quiesced before its first turn, the branch event carries none and says so, and
the branch's first measurement is the lineage's first, with nothing to compare against.
The tuple beyond that, the device, the kernel stack, the batch and the sampler, is the
deposit's box facts, as the result notes record it. **The weights check is a reader's.**
It compares the branch's first `model.measurement`, which the harness splices opaque per
`weaver-trace-Spec` section 3, against the expectation the branch event carries, and
reports a divergence, since the SPU holds no expectation to compare against. Every later
reader can walk from any run to the position it grew from, which is the lineage the
analysis-web contract's branch position already expects a record to state.

**That departs from a standing ruling, and the departure is the operator's to make.**
Under `weaver-state-PRD` section 4 today, a cut lands as a branch under a new session
name, and a cut under the record's own session name is refused before the preload,
because a session cannot rewind under its own name while its record carries what the cut
would drop. A same-session continuation keeps the abandoned line in the session's record
and its holdings, so the ruling changes with it, or the branch takes a new session name
and the lineage rides the branch event across sessions.

**The reload is exact on the tuple that ran and approximate across a card.** The
determinism matrix of 2026-09-27 and 2026-09-28 is the measurement: 464 of 464 turn
slots identical across the two A6000 cards of one tuple, and across card generations 20
of 464 differing between the A6000 pair and the Blackwell and 21 of 464 between the Ada
and either. So a branch on the same tuple is a controlled experiment, what differs after
N being what was changed, and a branch on another card is a run whose first divergence
may be the card's. The branch event carries what the record holds of the tuple, and the
deposit's box facts carry the rest, so the reader can tell which.

**Nothing in the SPU rewinds.** `weaver-spu-Spec` section 4 holds the session
append-only, `resident_len` and truncation a fault. So the branch is never a cache
operation on the running session: it is a reload, a fresh admission re-prefilled with
the context reconstructed to N from the record by that accumulation, and the running
session either continues on its own line or is unloaded. That is what keeps the record
honest, because the state the new run starts from is one the record can show, not one a
cache held.

**What the standing door reproduces, and its limit.** The restore of
`weaver-harness-Spec` section 6.1 recalls every message through the cut and seats the
recalled conversation beside the identity as one prefix in the open. So it is exact only
for a run with no flush and no elision recorded before N, between turns included:
`weaver-harness-Spec` section 6 lets the seat's flush run between turns and records it
there, and `--through` cuts at a turn's close and excludes an edit recorded after it,
since it seats the conversation whole rather than replaying the edits that shaped what
the model held. The exact re-prefill, the reconstruction of section 0 with every
recorded flush and elision replayed, between-turn ones included, is a capability this
feature owes the harness, and section 5 lists it.

## 2. Rewinding the agent does not rewind the world

The record restores what the agent was presented. The world the agent's tools touched
has moved on: a HeroBench character has the position, inventory and equipment the
abandoned line left it with, and a filesystem has the files. A branch from N is
therefore exact on the agent's side and one of two things on the world's side, and the
sketch names which applies to each tool rather than letting the question be discovered
by a retry that behaves strangely.

- **The tool resets the world to its state at N.** HeroBench can: the environment
  creates a character, and a branch recreates it and replays the accepted actions to N,
  or restores the character block the record carries. The environment's answer to an
  accepted action carries the character block, recorded verbatim in the tool result,
  which is what makes the restore possible from the record alone. The reset covers
  everything the experiment reads, the environment's own log of the character included,
  which the HeroBench loop and driver read to judge a task, or the branch declares the
  log's divergence.
- **The record carries the world's state at every tool result, and the branch measures
  the divergence.** Where a tool cannot reset, the tool result's content is the evidence
  of what the world was, and a branch reports what it found the world to be against what
  the record said it was at N.
- **The shell tool has neither.** A branch there is a new experiment against a changed
  world and the branch event says so, so a reader does not take it for a retry.

**An interview changes nothing in the world.** Section 3 refuses every tool call while
the run is quiesced, so an interview is conversation and never work, and resume-clean
has nothing in the world to undo.

## 3. The researcher's interrupt, the first caller

The interrupt is a run-level state transition, not a message, and the back end
owns it. Its shape, settled against the corpus:

**Quiescence is admitted over admin.** It is administrative control of the run rather
than work, so it rides `weaver-admin-harness-contract`, which has two initiators and one
dial per verb already. On receipt the harness stops admitting work at the exchange
boundary: the gate exchange in flight completes with every turn its loop makes of it, no
new exchange is admitted after the request, in-flight forward passes and concurrent
organ work finish, and nothing is cancelled mid-flight. A loop's follow-up turns are
therefore never refused mid-exchange, which matters because a loop such as
`bravo_loop.py` fails its whole request when a follow-up turn refuses.
**Resume-with-interview takes effect at the exchange boundary too**: an admin directive
may arrive while a turn runs, per `weaver-harness-Spec` section 3, so an interview
exchange in flight completes under the mark before the run resumes, and no new exchange
is admitted between the resume's arrival and its taking effect. **Resume-clean is issued
only when no exchange is in flight.** It composes unload and load, and
`weaver-admin-harness-contract` section 3 has the leave refuse while a turn is in
flight, so that refusal is the guard and the operator reissues after the exchange
completes. **Every request already queued at the harness is refused as quiesced**, since
the gate admits concurrent exchanges and the harness serialises them, each refusal
returning by the path its request line came in on, and the harness declares the run
quiesced with an empty queue. **The quiescence position is the last closed turn boundary
together with every flush and elision recorded between that turn's close and the
quiescence event**, so a resumption from it is the exact re-prefill of section 1
whenever such an edit stands. **A run with no closed turn has its quiescence position at
its opening**, the load, and a branch from it starts from what that opening seated: the
identity, and whatever the load restored where it was a restoring load, per
`weaver-harness-Spec` section 6.1. **While quiesced the harness executes no tool**: a
call the model makes in an interview turn is recorded and refused as quiesced, so an
interview is conversation and never work. A half-written state is a state that never
existed and is not worth interviewing. The trace records a quiescence event carrying the
position, the wall clock instant and what was in flight when the request arrived, so a
clean stop reads differently from a stop that caught the loop mid-turn.

**The interview enters through the gate as ordinary traffic.** The gate is the sole work
ingress and it authenticates the researcher already, so a second channel would cost a
socket and a contract for nothing. **An interview turn is marked positively, by its
request, and timing marks nothing.** **Two values are minted with the interrupt, and
only one of them is a secret.** The interrupt's identifier is a public name: the harness
writes it on every event authored while the run is quiesced and on the closing list, so
it stands in the record and proves nothing. **The bearer capability is a separate
value**, 128 bits drawn from the harness's entropy source, returned only in the
quiescence verb's answer over admin, presented on the client's request line, checked by
the harness, single-use per interrupt, and never written to the record, so holding it is
the proof of having asked for the quiescence, which is what the mark needs because the
gate admits any authorised principal. A mark that was the capability would put the
unspent secret on the quiescence event for any reader of the record. The harness admits
an interview request only while the run is quiesced and only when its line carries that
capability, and refuses it otherwise, so the mark binds to the quiescence act rather
than to any principal the gate admits, on the pattern of
`weaver-admin-harness-contract`'s "what crosses is a capability rather than a name", and
while the run is quiesced it refuses a request carrying none as quiesced. **Every event
the harness authors while the run is quiesced carries the interrupt's identifier**,
however many turns the loop makes of one request and including the turnless events
between them, since a loop may flush and recall between turns of one request as
`bravo_loop.py` does. The field that carries the capability on the request line is a
Spec election owed, the line's format being the Spec's under
`weaver-gate-world-contract`. The trace is turn-bracketed, so the turn is the unit that
is marked, and a replay presents the run with the interview or without it by that mark
on every event, and a count reports both.

**Cleanliness is read rather than inferred.** The run's closing event carries
the list of interrupt identifiers it saw, the empty list being the positive
uninterrupted marker.

**Resumption takes two forms and the operator elects between them.**
Resume-with-interview releases the loaded agent from quiescence with its cache warm, the
interview now part of the run by construction and marked as such. Resume-clean is two
verbs the operator issues in turn, an unload of the quiesced run and then a load of the
branch, because apex section 6 forbids one verb auto-chaining another, and the load is
section 1's door with a filter: a reload to the quiescence position with the interview
turns excluded, a new run whose branch event names the interrupt it resumes from. Either
way the interview is recorded, since an interview that left no mark on the resumed line
is still evidence about the agent.

**The lifecycle gains two verbs and a run-level condition.** Quiesce and
resume-with-interview join load, unload and validate in apex section 6, resume-clean
composing unload and load rather than being a verb of its own, under its rule that no
verb auto-chains another, and **quiescence is a run-level condition orthogonal to the
lifecycle state, read beside it**. The state stays `Idle` or `Active` as today, because
`weaver-harness-Spec` section 3 makes `Active` the run's own fact for exactly a turn's
extent, so a fifth exclusive case of the agent's states could not hold the run quiesced
through an interview turn. The observation answers the condition beside the state. That
is the operator's ruling at the apex, with `weaver-admin`, `weaver-harness`,
`weaver-trace` and `weaver-types` following in one act, contracts included.

## 4. The frontend

The fog toggle sets the full map against the map the agent has been shown. **The agent's
map is derivable from the record**, the game dump it was given riding the task's user
turn. **The full map is not in the record**: it is the environment's table, so showing
it needs a data path from the environment to the web seam, which today has no party for
it. That path is section 6's cell rather than a claim that the web contract alone
carries the toggle.

## 5. What lands where

- The apex, section 6: the two new verbs, quiesce and resume-with-interview, with
  resume-clean composing unload and load, and quiescence as a run-level condition read
  beside the lifecycle state rather than a case of it, the operator's ruling asked in
  those terms.
- `weaver-admin-harness-contract`, `weaver-admin-PRD` and Spec: the quiescence and
  resume exchanges and their refusals.
- `weaver-harness-PRD` and Spec: the boundary behaviour, the interview admitted by the
  capability its request carries and marked by the interrupt's identifier, and no tool
  run while quiesced, the two resumptions, the branch as a preload with a position.
- `weaver-gate-Spec` and then `weaver-harness-Spec`: the bearer capability on the
  client's request line, checked and never recorded. The gate Spec's section 4 holds the
  line's member list, one required member, `text`, an unknown member refusing the turn,
  so the field is that Spec's election first and the harness's admission second.
- `weaver-harness-Spec`, `weaver-harness-spu-decode-contract`, `weaver-spu-Spec`,
  `weaver-analysis-Spec` and `weaver-harness-state-contract`, every party in one act:
  the exact re-prefill of section 1, the reconstruction replayed with its recorded
  flushes and elisions into the open. The decode contract's section 2 admits a re-feed
  of recorded rendered contributions only where the instruction carries a re-feed
  permission, which admin sets from a diagnostic binding, so a serving harness writes no
  such ask today. `weaver-analysis-Spec` section 4's `--through` cuts at a turn's close
  and excludes a between-turn edit, and the preload's election is the state contract's.
- `weaver-trace-Spec` section 3: the quiescence event, the branch event with its world
  disposition, the interrupt's public identifier on every event authored while quiesced
  and on the closing list, and no field for the capability.
- `weaver-types-Spec`: the quiescence condition beside `AgentState` in the observation's
  answer, the state's cases unchanged.
- The web contract: the branch lineage as presented, and the fog toggle's agent-side
  map.

**The owners listed are as read on 2026-09-29.** An act's extent is what the act reaches
when it is written, so this list is a reading and not a claim of completeness, and the
mechanics are hardened in those acts and not here.

## 6. Open cells
- The full map's data path to the web seam, for the fog toggle: the environment's table
  is not in the record, and today no party carries it to the frontend.
- The exact re-prefill of section 1, which replays the recorded flushes and elisions
  that the standing restore seats whole: an act reaching the decode contract and
  `weaver-spu-Spec` beside the harness, under the apex ruling with the verbs.
- Continuation from a position inside a turn, which the door of section 1 does not
  take: a capability of its own, and the operator's.
- Whether a branch's parent is named by run identifier and sequence alone, or also by
  the hash of the context reconstructed at N, so a branch against an edited record
  refuses rather than continues.
- What "in flight" enumerates on the quiescence event: the forward pass, a tool
  call, an organ exchange, or any of them.
- Whether resume-with-interview and a warm-cache continuation after a plain
  quiescence are one verb or two.
- The diagnostic replay sketch's mid-run entry cell, whether entry at N matches a
  full replay at N, is this door's measurement and is not in the repository.
- The cost of a reload to N, which is a re-prefill and is measured rather than
  guessed.
