# weaver-state Spec: flow, schema, read, and persistence amendment

**Status:** MERGED.

**Landing PR:** #244

**Version:** 0.22 **Date filed:** 2026-10-20 **Parent:** weaver-state-PRD

## 0. What this document is

Replace serving Spec sections 2, 3, 4, and 6, and add section 5 instruments. Serving
owns state-content/1 and no caller DDL. Diagnostic retains the old database, election,
preload door and eight asks. They are separate modes and connections. No serving reader
uses event or field after the qualified foundation code cutover. The document merge
alone does not retire the existing serving implementation. [SEAM-1, SCOPE-1, R8]

## 1. Schema and filing, destination section 3

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**STATE-SCHEMA.** engine/serving_sqlite.rs owns the following DDL. Keep the old
engine/sqlite.rs and Store trait for diagnostic mode only. Add ServingStore in store.rs
for the typed file, query, image and adoption methods below. The mode enum selects one
engine and parser before reading fd3. Execute each fenced SQL statement separately.
Foreign keys are enabled only on this new serving connection. No migration mixes these
tables with diagnostic tables. SQLite internal row IDs are positive signed 64-bit
integers, never trace counters. Counter columns below are 8-byte big-endian unsigned
values. Column checks on length are part of the schema. Exhausted internal IDs refuse a
write, never wrap. Reason: the store owns its contract and preserves source order
without signed trace-counter casts. [STORE-3, STORE-5, SEAM-1]

**STATE-CONNECTION.** Construct every seed and serving connection with
rusqlite::Connection::open_in_memory. Apply foreign_keys=ON, journal_mode=MEMORY,
synchronous=OFF and temp_store=MEMORY as separate PRAGMA statements before schema
creation or adoption checks. After image adoption, reapply and read back those settings
before exposing the store. Require foreign_keys 1, journal_mode memory, synchronous 0
and temp_store 2. Require PRAGMA compile_options to report TEMP_STORE=1, TEMP_STORE=2 or
TEMP_STORE=3. Refuse an absent or contradictory build fact, or any failed setting, as
open_failed during serving open or write_failed during seed. Never fall back to a
file-backed connection or temporary store. [STORE-1, PERSIST-5, PERSIST-6]

The baseline Sqlite::stand already uses an in-memory connection, memory journal and
synchronous OFF. The new connection also fixes temporary query storage. Reason: an
in-memory main database alone does not elect where transient sort tables and indexes
live. SQLite documents both the temporary-store setting and its build override in
[PRAGMA temp_store][sqlite-temp]. This initialization serves STORE-1, while OS paging
and memory-pressure costs still require Spec 09's measurement. A successful setting
check cannot certify no disk wait. [STORE-1, FALS-3, COST-2]

```sql
CREATE TABLE store_meta (
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
  schema_id TEXT NOT NULL CHECK(schema_id = 'state-content/1'),
  seed_id TEXT NOT NULL UNIQUE,
  scheme_id TEXT NOT NULL
)
```

```sql
CREATE TABLE content_block (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  source_kind TEXT NOT NULL CHECK(source_kind IN ('seed','event')),
  seed_ordinal BLOB CHECK(seed_ordinal IS NULL OR length(seed_ordinal) = 8),
  run TEXT,
  sequence BLOB CHECK(sequence IS NULL OR length(sequence) = 8),
  block BLOB CHECK(block IS NULL OR length(block) = 8),
  session TEXT, turn TEXT, kind TEXT, channel TEXT, origin TEXT,
  envelope TEXT, rule_id TEXT, source_path TEXT,
  context_path TEXT, context_raw TEXT, effect TEXT,
  admission TEXT NOT NULL CHECK(admission IN ('seeded','admitted')),
  raw_block TEXT NOT NULL, statement_text TEXT, scheme_id TEXT NOT NULL,
  content_type TEXT NOT NULL CHECK(content_type IN ('statement','pair')),
  CHECK((content_type = 'statement' AND statement_text IS NOT NULL)
    OR (content_type = 'pair' AND statement_text IS NULL
      AND source_kind = 'event')),
  CHECK((context_path IS NULL AND context_raw IS NULL AND effect IS NULL)
    OR (context_path IS NOT NULL AND context_raw IS NOT NULL
      AND content_type = 'statement' AND source_kind = 'event'
      AND (effect IS NOT NULL OR (kind = 'plan.event' AND channel = 'input'
        AND origin = 'operator' AND block = X'0000000000000000')))),
  CHECK((source_kind = 'seed' AND seed_ordinal IS NOT NULL AND run IS NULL
    AND sequence IS NULL AND block IS NULL AND session IS NULL AND turn IS NULL
    AND kind IS NULL AND channel IS NULL AND origin IS NULL AND envelope IS NULL
    AND rule_id IS NULL AND source_path IS NULL AND admission = 'seeded')
    OR (source_kind = 'event' AND seed_ordinal IS NULL AND run IS NOT NULL
    AND sequence IS NOT NULL AND block IS NOT NULL AND session IS NOT NULL
    AND kind IS NOT NULL AND channel IS NOT NULL AND origin IS NOT NULL
    AND envelope IS NOT NULL AND rule_id IS NOT NULL AND source_path IS NOT NULL
    AND admission = 'admitted')),
  UNIQUE(run, sequence, block), UNIQUE(seed_ordinal)
)
```

```sql
CREATE TABLE statement_binding (
  id INTEGER PRIMARY KEY,
  block_id INTEGER NOT NULL REFERENCES content_block(id),
  ordinal BLOB NOT NULL CHECK(length(ordinal) = 8),
  start BLOB NOT NULL CHECK(length(start) = 8),
  end BLOB NOT NULL CHECK(length(end) = 8),
  verb TEXT NOT NULL,
  verb_start BLOB NOT NULL CHECK(length(verb_start) = 8),
  verb_end BLOB NOT NULL CHECK(length(verb_end) = 8),
  condition TEXT,
  condition_start BLOB CHECK(condition_start IS NULL OR length(condition_start) = 8),
  condition_end BLOB CHECK(condition_end IS NULL OR length(condition_end) = 8),
  CHECK((condition IS NULL AND condition_start IS NULL AND condition_end IS NULL)
    OR (condition IS NOT NULL AND condition_start IS NOT NULL AND condition_end IS NOT NULL)),
  UNIQUE(block_id, ordinal)
)
```

```sql
CREATE TABLE binding_noun (
  binding_id INTEGER NOT NULL REFERENCES statement_binding(id),
  ordinal BLOB NOT NULL CHECK(length(ordinal) = 8),
  value TEXT NOT NULL,
  start BLOB NOT NULL CHECK(length(start) = 8),
  end BLOB NOT NULL CHECK(length(end) = 8),
  PRIMARY KEY(binding_id, ordinal)
)
```

```sql
CREATE INDEX binding_verb ON statement_binding(verb COLLATE BINARY)
```

```sql
CREATE INDEX binding_condition ON statement_binding(condition COLLATE BINARY)
```

```sql
CREATE INDEX noun_value ON binding_noun(value COLLATE BINARY)
```

```sql
CREATE TABLE pair_event (
  id INTEGER PRIMARY KEY REFERENCES content_block(id),
  record_type TEXT NOT NULL CHECK(record_type IN ('started','ended'))
)
```

```sql
CREATE TABLE tool_pair (
  run TEXT NOT NULL,
  sequence BLOB NOT NULL CHECK(length(sequence) = 8),
  call BLOB NOT NULL CHECK(length(call) = 8),
  call_block BLOB NOT NULL CHECK(length(call_block) = 8),
  statement_id INTEGER NOT NULL REFERENCES statement_binding(id),
  opened_event INTEGER NOT NULL UNIQUE REFERENCES pair_event(id),
  lane TEXT NOT NULL CHECK(lane IN ('external','recall','review')),
  request TEXT NOT NULL,
  retry_of TEXT,
  fate TEXT NOT NULL CHECK(fate IN ('opened','completed','failed')),
  reason TEXT,
  terminal_event INTEGER UNIQUE REFERENCES pair_event(id),
  PRIMARY KEY(run, sequence, call),
  CHECK((fate = 'opened' AND reason IS NULL AND terminal_event IS NULL)
    OR (fate = 'completed' AND reason IS NULL AND terminal_event IS NOT NULL)
    OR (fate = 'failed' AND reason IS NOT NULL AND terminal_event IS NOT NULL))
)
```

```sql
CREATE INDEX pair_statement ON tool_pair(statement_id)
```

```sql
CREATE INDEX pair_open ON tool_pair(opened_event) WHERE fate = 'opened'
```

```sql
CREATE TABLE span (
  local_id BLOB NOT NULL CHECK(typeof(local_id) = 'blob' AND length(local_id) = 8),
  revision BLOB NOT NULL CHECK(revision = X'0000000000000001'),
  kind TEXT NOT NULL CHECK(kind IN
    ('system_prompt','task','task_prompt','subtask','step','step_outcome')),
  parent_local_id BLOB, parent_revision BLOB,
  status TEXT NOT NULL CHECK(status IN ('open','closed','retired')),
  opened_content_id INTEGER REFERENCES content_block(id),
  ended_content_id INTEGER REFERENCES content_block(id),
  replacement_local_id BLOB, replacement_revision BLOB,
  PRIMARY KEY(local_id, revision),
  FOREIGN KEY(parent_local_id, parent_revision) REFERENCES span(local_id, revision),
  FOREIGN KEY(replacement_local_id, replacement_revision)
    REFERENCES span(local_id, revision) DEFERRABLE INITIALLY DEFERRED,
  CHECK((parent_local_id IS NULL AND parent_revision IS NULL)
    OR (parent_local_id IS NOT NULL AND parent_revision IS NOT NULL)),
  CHECK((replacement_local_id IS NULL AND replacement_revision IS NULL)
    OR (replacement_local_id IS NOT NULL AND replacement_revision IS NOT NULL
      AND status = 'retired')),
  CHECK((kind = 'system_prompt' AND local_id = X'0000000000000000'
      AND parent_local_id IS NULL AND status = 'closed'
      AND opened_content_id IS NULL AND ended_content_id IS NULL)
    OR (kind != 'system_prompt' AND local_id > X'0000000000000000'
      AND opened_content_id IS NOT NULL
      AND ((status = 'open' AND ended_content_id IS NULL)
        OR (status IN ('closed','retired') AND ended_content_id IS NOT NULL
          AND ended_content_id >= opened_content_id))))
)
```

```sql
CREATE UNIQUE INDEX span_open_root ON span((1))
WHERE parent_local_id IS NULL AND status = 'open'
```

```sql
CREATE UNIQUE INDEX span_open_child ON span(parent_local_id, parent_revision)
WHERE parent_local_id IS NOT NULL AND status = 'open'
```

```sql
CREATE UNIQUE INDEX span_task_prompt ON span(parent_local_id, parent_revision)
WHERE kind = 'task_prompt'
```

```sql
CREATE INDEX span_parent ON span(parent_local_id, parent_revision, local_id)
```

```sql
CREATE TABLE span_prefix (
  local_id BLOB NOT NULL, revision BLOB NOT NULL,
  ordinal BLOB NOT NULL CHECK(length(ordinal) = 8),
  block_id INTEGER NOT NULL REFERENCES content_block(id),
  start BLOB NOT NULL CHECK(length(start) = 8),
  end BLOB NOT NULL CHECK(length(end) = 8 AND end > start),
  step_query TEXT,
  PRIMARY KEY(local_id, revision, ordinal),
  FOREIGN KEY(local_id, revision) REFERENCES span(local_id, revision)
)
```

```sql
CREATE TABLE span_member (
  local_id BLOB NOT NULL, revision BLOB NOT NULL,
  ordinal BLOB NOT NULL CHECK(length(ordinal) = 8),
  block_id INTEGER NOT NULL REFERENCES content_block(id),
  start BLOB NOT NULL CHECK(length(start) = 8),
  end BLOB NOT NULL CHECK(length(end) = 8 AND end > start),
  step_query TEXT,
  PRIMARY KEY(local_id, revision, ordinal),
  FOREIGN KEY(local_id, revision) REFERENCES span(local_id, revision)
)
```

The seed owns local_id 0, revision 1, kind system_prompt, status closed and null
parent/open/end/replacement fields. Its single prefix and finalized member both name the
complete prompt block from byte zero through its end mark. They are references, not
additional content blocks. SpanRef is seed_id, local_id, revision. Spec 06 owns the mark
operations below. Pair tables above are also part of this initial catalog, with
transitions owned by Spec 05. The plan catalog below is part of the same initial schema.
[PROMPT-1, PERSIST-5, MARK-1, MARK-5, PAIR-1]

**STATE-OBSERVATION-SCHEMA.** Operation metadata has its own tables in the same
in-memory state-content/1 catalog. It is never a content_block, statement binding, mark
member or tool pair. The seed creates these tables empty. JSON holds the strict
canonical TYPES-OBSERVATION values, with every identity checked before insertion. The
metadata key is the open run plus operation_id, not a content row or trace sequence.
Reason: measurements survive ordinary dumps without becoming model content. [STORE-1,
STORE-5, COST-2, R20-19]

```sql
CREATE TABLE observation_run (
  run TEXT PRIMARY KEY,
  member_observation TEXT NOT NULL
)
```

```sql
CREATE TABLE operation_observation (
  run TEXT NOT NULL REFERENCES observation_run(run),
  operation_id BLOB NOT NULL CHECK(typeof(operation_id) = 'blob'
    AND length(operation_id) = 8 AND operation_id > X'0000000000000000'),
  request_id BLOB NOT NULL CHECK(length(request_id) = 8),
  operation TEXT NOT NULL CHECK(operation IN
    ('open','file','file_pair','read','next','release','shape','held','position',
     'grants','snapshot','acknowledge','abort','release_capture')),
  source_run TEXT, source_sequence BLOB, source_block BLOB,
  observation TEXT NOT NULL,
  PRIMARY KEY(run, operation_id),
  CHECK((operation IN ('file','file_pair') AND source_run IS NOT NULL
      AND source_sequence IS NOT NULL AND length(source_sequence) = 8
      AND source_block IS NOT NULL AND length(source_block) = 8)
    OR (operation NOT IN ('file','file_pair') AND source_run IS NULL
      AND source_sequence IS NULL AND source_block IS NULL))
)
```

source fields identify the requested filing even when it was refused, so they carry no
foreign key to content. Typed validation checks their SourceBlockId grammar and matches
the request. The observation JSON must match operation_id, request_id and operation
exactly. observation_run stores the once-sampled MemberObservationIdentity for that run.
Opening an already recorded run refuses open_failed rather than appending to its
identity. A failed open before a validated run has no adopted metadata sink and is
reported on the existing pre-load diagnostic path. [FLOW-3, FLOW-4, STORE-5, COST-2,
R20-19]

**STATE-PLAN-SCHEMA.** The same serving connection owns the following additions. All
plan and step counters use the existing unsigned eight-byte encoding, starting at one.
JSON columns contain the foundation's canonical encoding of the strict Spec 07 types,
never executable SQL or a second instruction text. The seed has no plan rows. Reason:
stable IDs, review evidence and marks must survive the same dump as their statements.
[PLAN-1, PLAN-2, PLAN-6, STORE-5, PERSIST-1]

```sql
CREATE TABLE plan_revision (
  plan_id BLOB NOT NULL CHECK(typeof(plan_id) = 'blob' AND length(plan_id) = 8),
  revision BLOB NOT NULL CHECK(typeof(revision) = 'blob' AND length(revision) = 8),
  task_id BLOB NOT NULL, task_revision BLOB NOT NULL,
  prompt_id BLOB NOT NULL, prompt_revision BLOB NOT NULL,
  status TEXT NOT NULL CHECK(status IN
    ('authorized','locked','broken','complete','superseded')),
  operator_source INTEGER NOT NULL REFERENCES content_block(id),
  lock_source INTEGER REFERENCES content_block(id),
  last_source INTEGER NOT NULL REFERENCES content_block(id),
  review_setup TEXT NOT NULL, group_drafts TEXT NOT NULL, broken TEXT,
  PRIMARY KEY(plan_id, revision),
  FOREIGN KEY(task_id, task_revision) REFERENCES span(local_id, revision),
  FOREIGN KEY(prompt_id, prompt_revision) REFERENCES span(local_id, revision),
  CHECK(plan_id > X'0000000000000000' AND revision > X'0000000000000000'),
  CHECK((status = 'authorized' AND lock_source IS NULL)
    OR status = 'superseded'
    OR (status IN ('locked','broken','complete') AND lock_source IS NOT NULL)),
  CHECK((status = 'broken' AND broken IS NOT NULL)
    OR status = 'superseded' OR (status != 'broken' AND broken IS NULL))
)
```

```sql
CREATE UNIQUE INDEX plan_one_unfinished ON plan_revision((1))
WHERE status IN ('authorized','locked','broken')
```

```sql
CREATE TABLE plan_category (
  plan_id BLOB NOT NULL, revision BLOB NOT NULL,
  category_id TEXT NOT NULL,
  ordinal BLOB NOT NULL CHECK(length(ordinal) = 8),
  category TEXT NOT NULL,
  PRIMARY KEY(plan_id, revision, category_id),
  UNIQUE(plan_id, revision, ordinal),
  FOREIGN KEY(plan_id, revision) REFERENCES plan_revision(plan_id, revision)
)
```

```sql
CREATE TABLE plan_group (
  plan_id BLOB NOT NULL, revision BLOB NOT NULL,
  group_id TEXT NOT NULL,
  ordinal BLOB NOT NULL CHECK(length(ordinal) = 8),
  span_id BLOB, span_revision BLOB,
  PRIMARY KEY(plan_id, revision, group_id),
  UNIQUE(plan_id, revision, ordinal),
  FOREIGN KEY(plan_id, revision) REFERENCES plan_revision(plan_id, revision),
  FOREIGN KEY(span_id, span_revision) REFERENCES span(local_id, revision),
  CHECK((span_id IS NULL AND span_revision IS NULL)
    OR (span_id IS NOT NULL AND span_revision IS NOT NULL))
)
```

```sql
CREATE TABLE plan_step (
  plan_id BLOB NOT NULL, revision BLOB NOT NULL,
  step_id BLOB NOT NULL CHECK(typeof(step_id) = 'blob' AND length(step_id) = 8
    AND step_id > X'0000000000000000'),
  ordinal BLOB NOT NULL CHECK(length(ordinal) = 8),
  instruction_id INTEGER NOT NULL REFERENCES statement_binding(id),
  category_id TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('waiting','active','closed','retired')),
  span_id BLOB, span_revision BLOB,
  outcome_id BLOB, outcome_revision BLOB,
  review_run TEXT, review_sequence BLOB, review_call BLOB,
  work TEXT, closure_source INTEGER REFERENCES content_block(id),
  PRIMARY KEY(plan_id, revision, step_id),
  UNIQUE(plan_id, revision, ordinal),
  FOREIGN KEY(plan_id, revision) REFERENCES plan_revision(plan_id, revision),
  FOREIGN KEY(plan_id, revision, category_id)
    REFERENCES plan_category(plan_id, revision, category_id),
  FOREIGN KEY(span_id, span_revision) REFERENCES span(local_id, revision),
  FOREIGN KEY(outcome_id, outcome_revision) REFERENCES span(local_id, revision),
  FOREIGN KEY(review_run, review_sequence, review_call)
    REFERENCES tool_pair(run, sequence, call),
  CHECK((span_id IS NULL AND span_revision IS NULL)
    OR (span_id IS NOT NULL AND span_revision IS NOT NULL)),
  CHECK((status = 'waiting' AND span_id IS NULL) OR status = 'retired'
    OR (status IN ('active','closed') AND span_id IS NOT NULL)),
  CHECK((status = 'closed' AND outcome_id IS NOT NULL
      AND outcome_revision IS NOT NULL
      AND ((review_run IS NULL AND review_sequence IS NULL AND review_call IS NULL)
        OR (review_run IS NOT NULL AND review_sequence IS NOT NULL
          AND review_call IS NOT NULL))
      AND work IS NOT NULL AND closure_source IS NOT NULL)
    OR (status != 'closed' AND outcome_id IS NULL AND outcome_revision IS NULL
      AND review_run IS NULL AND review_sequence IS NULL AND review_call IS NULL
      AND work IS NULL AND closure_source IS NULL))
)
```

```sql
CREATE UNIQUE INDEX plan_one_active ON plan_step((1)) WHERE status = 'active'
```

```sql
CREATE TABLE plan_group_step (
  plan_id BLOB NOT NULL, revision BLOB NOT NULL,
  group_id TEXT NOT NULL, step_id BLOB NOT NULL,
  PRIMARY KEY(plan_id, revision, step_id),
  FOREIGN KEY(plan_id, revision, group_id)
    REFERENCES plan_group(plan_id, revision, group_id),
  FOREIGN KEY(plan_id, revision, step_id)
    REFERENCES plan_step(plan_id, revision, step_id)
)
```

**STATE-PLAN-FILE.** Apply Spec 07's exact PlanEffect within STATE-FILE after its source
statement has been inserted. All failures refuse invalid_effect and roll back the
source, plan and marks together. Operator-input plan.event may hold copied
PlanInputContext without an effect. No other new context-only filing is licensed. Check
canonical field shapes, current seed, statement references, group order, category
membership and the source kind/channel/origin before the guards below. The store
verifies structural evidence, not the review tool's truth or the model's intent. The
harness owes preflight so an ordinary invalid operator proposal is an operation refusal
before filing an effect. [PLAN-2, PLAN-5, PLAN-6, CLASS-10]

| Effect | Prior-state guard | Transaction |
| --- | --- | --- |
| authorize | No authorized or locked plan, next PlanId/revision one, source is block zero of a fully held operator authorize event | Begin task scope, insert authorized revision and its categories/drafts, no steps |
| lock from operator | No authorized or locked plan, next PlanId/revision one, source is fully held create event | Begin task scope, insert locked revision, categories, groups and waiting steps |
| lock from model | Exact current authorized revision and source, unchanged task/review/categories/drafts, all candidate statements held | Insert steps/groups, guard authorized to locked, record this effect as lock source |
| replace | Exact current expected revision, next revision exactly plus one, source is fully held replace or answer_break event | Retire open execution scopes, guard old revision to superseded, insert next revision and rows, preserve only verified closures |
| activate | Current locked revision, no active step, named waiting step is first waiting ordinal, no earlier unclosed step, successor matches STATE-STEP-QUERY | Open group if needed, open step, set active with its new SpanRef |
| close | Current locked revision and active step, current WorkRef and the category-required evidence, next outcome SpanRef | Create/close outcome, close step, set closed, close complete ancestors and set plan complete if no waiting step remains |
| break | Current locked revision and matching active step or null when none, proposed source held when present | Set broken and this effect's source as last_source, no mark closure |

A fully held operator event means every Text index named in its PlanInputContext has a
matching content row and the task or step constraints in Spec 07 hold. Its block-zero
source is plan.event/input/operator, with null effect. An effect source is
plan.event/internal/loop except close, which is step.outcome/internal/loop. Its context
equals the typed effect. lock_source names the effect that made the revision locked, not
a candidate message. last_source advances on each effect. Categories and ordered group
drafts come only from the operator context. Model lock may not amend them. [PLAN-1,
PLAN-5, PLAN-6, STORE-3]

For create or authorize after a broken revision, retire its open scopes leaf first and
set that revision superseded in the same transaction before inserting the new plan. The
plan_one_unfinished index remains. Complete history is left complete. Do not clear the
old rows or disable the uniqueness check. The new operator context's review identity is
stored verbatim and cannot be supplied by a model lock. [PLAN-1, PLAN-5, PLAN-6, R20-12]

New plans use MAX(plan_id)+1, revision one, with an empty-table base of zero. New steps
use MAX(step_id)+1 across all revisions, also from zero. Existing step IDs in a
replacement must occur in its immediately preceding revision. A removed ID cannot return
in a later revision. Reordered IDs retain their identity, and their new ordinal is the
supplied order. Statement slices use binding start/end bytes, not an assumed whole
source block for a model candidate. [PLAN-1, STORE-5]

A model lock's candidate is the exact ordered list held by the harness's private
EffectiveEmission grant. Its valid corrections may come from later model emissions. The
store requires unique held StatementRefs, exact equality to the new steps' instruction
list and the current authorization. It does not invent a same-emission predicate or
claim to reconstruct the repair proof from statement rows. That proof is enforced by the
harness before it authors the effect. [PLAN-1, PLAN-6, CLASS-9, STORE-3]

For create, authorize or a changed task, retire any old open path leaf first and begin a
new task and prompt with the designated operator task block. A replacement without task
text keeps its open TaskScopeRef. If that task is terminal, create a new task and prompt
using its already held prompt prefix, never reopening it. New scopes use the next mark
IDs and revision one. A changed or renewed task invalidates the old task placement
grant. A later placement must keep no changed-task suffix, under Spec 06. The store
issues no cut. The mark helper accepts the held operator source, or the old prompt
reference in the latter case, only inside this operator effect. [PLAN-6, PLACE-2,
MARK-1, MARK-5]

Replacement retires an active step and its open group before inserting the new revision.
It never relabels them closed. Preserve a requested closure only when that StepId is
closed in the prior revision, its instruction bytes and complete Category value are
unchanged, and review setup and stored evidence match. Reuse the prior held instruction
reference for identical bytes. Copy its exact terminal span, outcome, review pair,
WorkRef and closure source. Those historical spans keep their parents. The new plan's
groups describe logical membership, while group.span names work executed in this
revision. A group with only preserved steps needs no new execution span. A partly
preserved group opens its new span at its first waiting step and holds only the new
execution. No mark is reparented or fabricated for old work. All omitted or nonpreserved
steps are waiting in the new revision. If none wait, close any still-open task under the
effect's source and set complete. [PLAN-2, PLAN-6, MARK-6]

For activation use the first waiting ordinal. Its group must be absent or the only open
group for that task. Use Spec 06's instruction prefix and child guards. Closing reads
the exact held Category.review_required. If review_call is null, require false, no
review ID fields, and the private attest_closure grant on the harness side. Otherwise
require lane review, fate completed, Result outcome, exact request.setup and
request.work equal to the plan's setup and effect.work. A partial review ID refuses.
WorkRef must match the current plan, step and active span in both cases. Recompute its
preview/digest and require unchanged work. For a reviewed close the frontier predates
the review request, allowing only that request, its pair and this outcome afterward. For
an unreviewed close only this outcome may follow the work frontier. The outcome label is
closed. The harness verifies trusted authority, while the store verifies this structural
account. [PLAN-2, MARK-4, STORE-5, PAIR-3, R20-17]

The guarded step UPDATE below must change exactly one row. It is executed in the same
transaction as the outcome/step marks. All bound IDs are resolved from typed references
first. A stale, duplicate or nonactive closure cannot succeed. [PLAN-2, MARK-5, SEAM-2]

```sql
UPDATE plan_step AS s
SET status = 'closed', outcome_id = :outcome_id,
    outcome_revision = :outcome_revision,
    review_run = :review_run, review_sequence = :review_sequence,
    review_call = :review_call, work = :work, closure_source = :source_id
WHERE s.plan_id = :plan_id AND s.revision = :revision AND s.step_id = :step_id
AND s.status = 'active'
AND EXISTS (SELECT 1 FROM plan_revision p
  WHERE p.plan_id = s.plan_id AND p.revision = s.revision AND p.status = 'locked')
AND EXISTS (SELECT 1 FROM plan_category c
  WHERE c.plan_id = s.plan_id AND c.revision = s.revision
  AND c.category_id = s.category_id
  AND ((json_extract(c.category, '$.review_required') = 0
      AND :review_run IS NULL AND :review_sequence IS NULL AND :review_call IS NULL)
    OR (:review_run IS NOT NULL AND :review_sequence IS NOT NULL
      AND :review_call IS NOT NULL
      AND EXISTS (SELECT 1 FROM tool_pair t
        WHERE t.run = :review_run AND t.sequence = :review_sequence
        AND t.call = :review_call AND t.lane = 'review' AND t.fate = 'completed'))))
```

Plan status changes also compare the exact expected previous status/revision and require
one changed row. After a close, all grouped steps closed closes its open subtask. All
steps closed closes its task and sets plan complete. Close child first, with canonical
membership and fixed pair frontier, before the receipt. A failed review produces a held
step.outcome with no close effect and changes none of these rows. [PLAN-2, SERVE-3,
MARK-1]

**STATE-PLAN-READ.** The plan_current selector uses the following query. The second
query is historical lookup after checking the seed. Read categories/groups by their
ordinal and steps by their ordinal, joining group membership by its unique step key.
Resolve instruction_id through statement_binding and content_block back to StatementRef,
and all content IDs back to source identities. For the returned last_source,
resolve its content key to SourcePosition with the stored source's exact nullable
turn. Resolve operator_source to its held PlanInputContext and return that
context's document_sha256. Both facts belong to the same immutable result as the
revision, including historical plan reads and reload. The member owns consistency
with these held sources, under existing adoption and read validation. The harness
cannot independently check a well-shaped false boot digest or turn without an
already-held trusted expectation. Do not derive either from caller input or the
current run. This is PLAN-SUMMARY-EVIDENCE and changes no SQL schema or stored
source key. Decode canonical typed JSON strictly. Never
expose SQL keys. The result has Spec 07's exact PLAN-READ shape. Reason: one store-owned
read supplies every consumer's stable step and review account. [PLAN-1, PLAN-2, SEAM-3,
STORE-5]

```sql
SELECT * FROM plan_revision
WHERE status != 'superseded' ORDER BY last_source DESC LIMIT 1
```

```sql
SELECT * FROM plan_revision WHERE plan_id = :plan_id AND revision = :revision
```

```sql
SELECT s.*, g.group_id FROM plan_step s
LEFT JOIN plan_group_step g ON g.plan_id = s.plan_id
  AND g.revision = s.revision AND g.step_id = s.step_id
WHERE s.plan_id = :plan_id AND s.revision = :revision ORDER BY s.ordinal
```

Compute next IDs from unsigned MAX columns with checked addition in Rust. Complete
results use the existing snapshot/page bound. Adoption validates contiguous plan and
step allocations, sequential revisions, current-plan and active-step uniqueness, all
source/effect/mark references, categories and group membership, and terminal
review/closure correspondence. Historical preserved evidence stays in the old revision
as well. It is never rewritten to make a new revision look reviewed. [PLAN-1, PLAN-2,
PERSIST-6]

**STATE-PLAN-FLAGS.** plan_flags is a read-only aggregate with Spec 07's exact
PLAN-FLAG-READ result. In one read transaction, check store_meta.seed_id, resolve the
plan and source, and compare the requested plan/after against STATE-PLAN-READ's current
revision and last_source. Refuse wrong_seed, not_found or invalid_selector as that
contract states. Resolve position in the same transaction for through. This selector
calculates no next ID and does not refuse merely because allocation is exhausted.
Reason: the flags and their possible targets must describe one boundary. [PLAN-3,
PLAN-6, STORE-5, SEAM-3, COST-4]

Read the nonretired target IDs with the first query. Decode the eight-byte IDs without
signed casts, then encode each as the canonical `step:<local_id>` string. Bind their
unique vector as the JSON array :step_conditions to the second query. An empty step
vector binds []. It is never interpolated SQL. The count ranges over the complete held
chain, one statement binding per row, without a noun join or a source-kind filter.
Conditions outside the canonical positive-u64 address grammar do not enter the count.
[PLAN-3, STORE-3, STORE-5, COST-4]

```sql
SELECT step_id FROM plan_step
WHERE plan_id = :plan_id AND revision = :revision AND status != 'retired'
ORDER BY ordinal
```

```sql
SELECT COUNT(*) AS total_flags,
  COALESCE(SUM(CASE WHEN t.value IS NULL THEN 0 ELSE 1 END), 0) AS matched_flags
FROM statement_binding b
LEFT JOIN json_each(:step_conditions) t ON b.condition = t.value COLLATE BINARY
WHERE b.condition GLOB 'step:[1-9]*'
AND substr(b.condition, 6) NOT GLOB '*[^0-9]*'
AND instr(b.condition, char(0)) = 0
AND length(b.condition) BETWEEN 6 AND 25
AND (length(b.condition) < 25
  OR b.condition <= 'step:18446744073709551615' COLLATE BINARY)
```

Convert both counts to DecimalU64 and derive unmatched_flags by checked subtraction.
Serialize PlanFlagSnapshot under the ordinary result byte limit before ending the read
transaction. Later filing cannot change the sample or its through frontier. Retired
historical steps are not made current to hide a count, and closed current steps are
still valid targets for this identity measure. No query writes an arming flag, alters a
condition or decides that a task missed needed content. The counts and actual read cost
go on the trace through their existing owners. [PLAN-3, SEAM-3, COST-4, SCOPE-1]

**STATE-FILE.** ServingStore::file(&StatementFile) returns FileCommit {block_id} or
FileFault using TYPES-WIRE codes. Verify source/envelope/path identity, open run and
session, Text raw_block, exact decoded text, UTF-8 spans, ordered nonempty statements,
nonempty verb and nouns, and condition nullability. Require filing.scheme_id to equal
both the successfully opened scheme_id and store_meta.scheme_id. Refuse scheme_mismatch
before any insertion if they differ. Filing a duplicate source refuses even if
identical. Structural validation never reclassifies meaning. [SEAM-2, STORE-2, STORE-3,
FLOW-4]

One transaction inserts content_block, every binding and every noun, then applies the
licensed effect. A plain statement has null context_path, context_raw and effect. A
licensed statement retains the copied context spelling and its typed effect as canonical
JSON. Operator plan-input context has null effect under STATE-PLAN-FILE. Commit returns
before constructing FileCommit. Refusal rolls everything back, including the source row.
raw_block retains its canonical source spelling. Text matching uses BINARY bytes with no
normalization, case folding or whitespace collapse. The noun and verb are classifier
keys, not inferred by the store. There is no duplicate index on the UNIQUE source
triple. The increasing internal content ID supplies order across loaded sessions,
inherited unchanged in each dump. Forks start with the parent's held prefix and grow
their own order. A repeated read never writes that order. [STORE-2 through STORE-5,
READ-1]

**STATE-MARK-FILE.** Spec 06's MarkEffect operations run inside STATE-FILE, never
through a direct mutation control. Decode SourceContext strictly, compare it with the
supplied effect, and check its kind/channel/origin/block licensing under TYPES-EFFECT.
The store checks the copied evidence and structural operation. It does not repeat the
classifier or decide that review passed. A mismatch or any failed guard below refuses
invalid_effect and rolls back the whole filing. Reason: a mark and the statement that
accounts for it commit together. [MARK-5, PLAN-2, SEAM-2, STORE-3]

Resolve every wire ContentRef and SpanRef against the loaded seed. Allocate new span
local IDs as checked MAX(local_id)+1, revision 1, advancing once per open inside this
transaction. IDs have no gaps or reuse. A prefix is a nonempty exact UTF-8 slice of a
filed statement at or before the open source, including this filing where the owning
effect permits it. Prefix ordinals start at zero. Refuse a slice that splits a binding,
with paired requests subject to the whole-pair guard as well. No prefix contains raw
tool metadata. [MARK-1, MARK-3, MARK-6]

| Operation | Required prior state and validation | Atomic effect |
| --- | --- | --- |
| begin_task on state.mark | No open task, source is an earlier filed message.user block from this turn, task and prompt are the next two IDs | Open task root, create closed task_prompt child whose prefix/member is the whole input block |
| begin_task inside operator plan update | Old open path retired by that effect, task source is its designated filed task block, next two IDs | The same task/prompt construction |
| open subtask | Current open task parent, no open child, empty prefix, next ID | Insert open span |
| open step | Current task or subtask at the open leaf, one held instruction prefix, next ID | Insert open span and its prefix |
| open step_outcome | Current open step at the leaf, one outcome statement prefix, next ID | Insert the child before the same plan effect closes it |
| close | Named span open, all children terminal, canonical membership and included pairs valid | Insert finalized members, set closed and current filing as end |
| retire | Same open/child/membership guards, optional replacement resolves by commit | Insert finalized members, set retired and administrative end, never a review label |
| Any operation on terminal span or foreign seed | Invalid | Refuse whole filing |

Only task may have a null runtime parent. task_prompt and subtask require task, step
requires task or subtask, and step_outcome requires step. All runtime spans belong to
the current task, and parents precede children in ID order. These checks plus the
partial unique indexes prevent cycles and crossed scopes. A replacement reference may be
forward within the same atomic operator update, but must resolve to another span of the
same kind by commit. It never changes the old membership. Spec 07 owns which authorized
plan operation may invoke these internal helpers. [MARK-1, MARK-5, PLAN-2, PLAN-6]

For close or retire, compute the canonical rows in STATE-MARK-MEMBERS using this
filing's content ID and strict ordinary-work end. Validate all rows and included pairs
before inserting contiguous member ordinals. Apply the following guarded update, require
exactly one changed row, and only then commit. A child closure in the same plan effect
must precede its parent's. Remove both membership and source on rollback. [MARK-4,
MARK-5, STORE-5]

```sql
UPDATE span AS s
SET status = :status, ended_content_id = :end_id,
    replacement_local_id = :replacement_id, replacement_revision = :replacement_rev
WHERE s.local_id = :span_id AND s.revision = :span_rev AND s.status = 'open'
AND NOT EXISTS (SELECT 1 FROM span child
  WHERE child.parent_local_id = s.local_id AND child.parent_revision = s.revision
  AND child.status = 'open')
```

The helper accepts only closed or retired as status. closed requires null replacement.
Prefix rows and finalized members cannot be updated or deleted by a serving operation.
Runtime closure sources always name the current effect's event block. The seed uses its
separate pre-agent transaction and no event source. [MARK-1, MARK-5, PERSIST-5]

**STATE-PAIR-FILE.** ServingStore::file_pair(&PairFile) returns the same FileCommit or
FileFault as file. It checks the open scheme, source/envelope identity, virtual block
zero, empty path and the raw payload against the typed pair. It inserts a content_block
with content_type pair and statement_text NULL, then pair_event and the guarded
tool_pair transition from Spec 05's PAIR-SCHEMA in one transaction. The complete
PairEnded is retained as raw_block, including every outcome field. Only a successful
commit permits filed. Statement/seed writes set content_type statement. A typed pair
never acquires classifier bindings. [SEAM-2, STORE-2, STORE-5, PAIR-1, PAIR-3]

The member checks referenced statements, lane/request agreement and the closed fate
mapping. It assumes the harness has proved the source statement's association with its
executable request, because a Text-only filing cannot independently reconstruct an
adjacent ToolCall that was not projected. Refuse an unknown or already terminal CallId
and roll back the entire new source row. Never treat a file_pair refusal as an ordinary
conformance outcome. [PAIR-1, PAIR-4, CLASS-8]

For a recall opening, require exactly one noun on the held binding, no governing
condition and subject bytes equal to that noun. Membership among several nouns is not
sufficient. A live filing refuses invalid_pair with full rollback on mismatch.
STATE-ADOPT applies the same shape check to saved pairs under its open refusal family.
The harness owns recall-key recognition
and the distinct RECALL-REFUSAL operation before pair creation. These store checks
enforce the existing valid recall shape, without a new pair variant, classifier call or
catalog change. [READ-3, PAIR-4, PERSIST-4, CLASS-2]

## 2. Read service and the old readers, destinations sections 3 and 4

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**STATE-READ.** ServingStore::query(&ReadSelector) returns QueryResult or
ReadRefusalCode. The read method takes &self and uses prepared SELECTs, never caller
SQL. History returns one entry per matching statement, all its nouns, its condition,
source and original block, in content ID then binding ordinal order. An entry is not
duplicated for two matching nouns. Filters are exact optional verb and condition keys.
Condition selects all statements with that exact condition. Scope is the whole held
chain, replacing the old per-session bound. The seed is queryable like other held
content. [READ-1, READ-4, STORE-3, STORE-4, PERSIST-7]

The following query templates choose IDs. Fetch their full blocks, bindings and nouns
within the same read operation. Bind parameters, including optional filters. `?` is a
value placeholder, never SQL text substitution. [SEAM-3, READ-1]

```sql
SELECT b.id, s.id FROM content_block b
JOIN statement_binding s ON s.block_id = b.id
WHERE s.id IN (SELECT n.binding_id FROM binding_noun n INDEXED BY noun_value
  WHERE n.value = ? COLLATE BINARY)
AND (? IS NULL OR s.verb = ? COLLATE BINARY)
AND (? IS NULL OR s.condition = ? COLLATE BINARY)
ORDER BY b.id, s.ordinal
```

```sql
SELECT b.id, s.id FROM content_block b
JOIN statement_binding s ON s.block_id = b.id
WHERE s.condition = ? COLLATE BINARY
ORDER BY b.id, s.ordinal
```

```sql
SELECT m.ordinal, b.id, m.start, m.end, m.step_query FROM span_member m
JOIN content_block b ON b.id = m.block_id
WHERE m.local_id = ? AND m.revision = ?
AND b.content_type = 'statement' ORDER BY m.ordinal
```

Span reads first check seed_id against store_meta and require a closed or retired span.
An open span refuses span_open. They return exact slices and the original blocks with
offsets. They never alter holdings. system_prompt is the same query for span 0 revision
1, with the prompt invariant checked. Later explicit span reads are legal, so there is
no store-side boot-only lock. The harness caches the mandatory boot read. [READ-5,
PERSIST-6, PROMPT-1, October 18 X05 disposition]

**STATE-SUBJECTS.** subjects takes the nonempty sorted unique noun list from Spec 08.
Validate the list before SQL. Bind its complete JSON string array as :nouns. This query
chooses whole statement blocks in the store's commit order. Run the block fetch, binding
fetch and pair joins in the same read transaction. No run ID is an order key. Reason:
automatic history must neither repeat a compound-object block nor recursively select the
loop's own bookkeeping. [READ-1, READ-2, STORE-4, SERVE-1]

```sql
SELECT b.id FROM content_block AS b
WHERE b.source_kind = 'event' AND b.content_type = 'statement'
AND b.kind IN ('message.user','message.assistant')
AND b.id IN (
  SELECT s.block_id FROM json_each(:nouns) AS q
  JOIN binding_noun AS n INDEXED BY noun_value
    ON n.value = q.value COLLATE BINARY
  JOIN statement_binding AS s ON s.id = n.binding_id
)
ORDER BY b.id
```

For each selected block, fetch every binding with the query below, not only the binding
that matched. Attach each binding's complete noun list and all its ReadPairs,
deduplicated by CallId and ordered by opening content ID. Expose no internal row ID.
This is one ReadEntry per block with selected_span null. The whole-block result must not
omit a tool pair merely because another statement supplied the matching noun. An empty
match is entries:{entries:[]}, and a read refusal is a different result. Explicit
history and condition queries retain their per-statement semantics. [STORE-3, STORE-5,
READ-1, READ-4, PAIR-4]

```sql
SELECT s.* FROM statement_binding AS s
WHERE s.block_id = :block_id ORDER BY s.ordinal
```

**STATE-STATEMENT.** statement resolves a StatementRef using the query below, with
sequence, block and ordinal bound as their 8-byte counter representations. It returns
one entry, the complete source Text block, one ReadBinding with its original ordinal,
and all pairs of that binding. No match is not_found. It cannot return the seed, which
has no event StatementRef. Reason: a plan instruction or forward flag names a statement,
and its ordinal must survive filtering. [PLAN-1, PLAN-3, STORE-3, READ-4]

```sql
SELECT b.id, s.id FROM content_block AS b
JOIN statement_binding AS s ON s.block_id = b.id
WHERE b.source_kind = 'event' AND b.content_type = 'statement'
AND b.run = :run AND b.sequence = :sequence AND b.block = :block
AND s.ordinal = :ordinal
```

Require SQLite JSON table support in serving and seed mode. Probe json_each against a
fixed two-string array before accepting the engine. A missing implementation refuses
engine admission, never an empty subjects result or a fallback full scan. Adoption still
verifies the same catalog and indices. The noun index is a required schema member, and
loss of it refuses adoption. [STORE-1, STORE-4, READ-2]

**STATE-MARK-MEMBERS.** The following SELECT is the canonical runtime membership
constructor for closure and span_preview. span_id/span_rev name the root span, open_id
is its opened_content_id, and upper_id is the closing filing's content ID or the
preview's resolved through content ID. inclusive_work is zero for closure and one for
preview. These are internal parameters, never caller SQL or wire row IDs. The recursive
walk follows only the checked acyclic parent relation. [MARK-1, MARK-3, MARK-5]

```sql
WITH RECURSIVE descendants(local_id, revision, opened_content_id) AS (
  SELECT local_id, revision, opened_content_id FROM span
  WHERE local_id = :span_id AND revision = :span_rev
  UNION ALL
  SELECT s.local_id, s.revision, s.opened_content_id FROM span s
  JOIN descendants d ON s.parent_local_id = d.local_id
    AND s.parent_revision = d.revision
), members AS (
  SELECT d.opened_content_id AS anchor, d.local_id AS prefix_span,
         p.ordinal AS prefix_ordinal, p.block_id, p.start, p.end, p.step_query
  FROM descendants d JOIN span_prefix p
    ON p.local_id = d.local_id AND p.revision = d.revision
  WHERE d.opened_content_id >= :open_id AND d.opened_content_id <= :upper_id
  UNION ALL
  SELECT b.id, NULL, NULL, b.id, NULL, NULL, NULL FROM content_block b
  WHERE b.id > :open_id
    AND (b.id < :upper_id OR (:inclusive_work = 1 AND b.id = :upper_id))
    AND b.content_type = 'statement'
    AND b.kind IN ('message.user','message.assistant','loop.instruction')
)
SELECT anchor, prefix_span, prefix_ordinal, block_id, start, end, step_query
FROM members
ORDER BY anchor, CASE WHEN prefix_span IS NULL THEN 1 ELSE 0 END,
         prefix_span, prefix_ordinal
```

For a row with null prefix_span, start/end are replaced with zero and the exact UTF-8
byte length of its statement. Otherwise decode their existing unsigned offsets. Assign
result ordinals from zero after sorting. A block appearing in distinct prefixes retains
those distinct occurrences. step_query is null for ordinary members and the seed. A step
prefix requires the canonical StepQueryRef from Spec 06. On activation, validate its
plan, predecessor, step and instruction against the held revision, then store it with
the prefix. Copy it unchanged into finalized membership and ReadEntry.step_query. It
never imports the predecessor's work. The ordering must match Spec 06's SPAN-ORDER,
including a prompt at its task's opening anchor and an outcome at its parent's closing
anchor. The seed uses its fixed member and never this runtime interval query. [MARK-3,
MARK-5, STORE-5]

**STATE-STEP-QUERY.** step_successor is a read against the exact PlanRef. Select the
first waiting step in ordinal order. No step may be active, and every earlier step must
be closed. previous_step must be null exactly when that waiting step has ordinal 1.
Otherwise it must name the closed step at the immediately preceding ordinal in that
revision. Preserved closed steps at later ordinals do not block the query. A terminal
plan or no waiting step returns not_found. A wrong predecessor or revision, an active
step or an earlier unclosed step returns invalid_selector. The result is
{reference,instruction}, with StepQueryRef from Spec 06 and the statement read for that
instruction. It never returns the previous step's work span. The store constructs the
reference, the caller cannot edit it into activation. Reason: preserved later closures
must not make the revised plan's first waiting step unreachable. [MARK-3, PLAN-4,
STORE-3, R20-10]

Use STATE-PLAN-READ's exact revision and ordered step query, then STATE-STATEMENT for
its stored instruction. These reads share the same snapshot. Annotate that entry with
the returned step_query, and require byte-for-byte equality at the activation write
guard. This is read-only, and no next-step action follows from returning it. The
acceptance fixture swaps predecessor, revision and instruction independently, each must
refuse when its guard is present and pass when that protective guard is removed. It also
replaces a plan with a new waiting ordinal 1 and a preserved closed ordinal 2, then
reads and activates ordinal 1 with null predecessor. A second fixture has closed ordinal
1, waiting ordinal 2 and preserved closed ordinal 3, and requires ordinal 1 as
predecessor. Reinstating the closed-prefix assumption must fail these fixtures, and
removing the first-waiting or no-active guard must admit a forbidden skipped or
concurrent step. [PLAN-1, PLAN-2, R20-12]

**STATE-MARK-PAIRS.** For each selected interval, inspect the block's bindings. Return
only wholly selected bindings. A partial intersection refuses the operation, and a
partial paired request is specifically partial_pair on read. For closure either is
invalid_effect. For each fully selected binding, use this query with the fixed frontier.
It is the whole-pair join of Spec 05 with an opening cutoff. [MARK-3, PAIR-3, PAIR-4]

```sql
SELECT p.*, o.envelope AS opening_envelope, o.raw_block AS opening_block,
       t.envelope AS terminal_envelope, t.raw_block AS terminal_block
FROM tool_pair AS p INDEXED BY pair_statement
JOIN content_block AS o ON o.id = p.opened_event
LEFT JOIN content_block AS t ON t.id = p.terminal_event
WHERE p.statement_id = :statement_id AND p.opened_event <= :frontier
ORDER BY p.opened_event
```

Before returning or finalizing any member, require every selected pair's terminal_event
to be nonnull and no greater than frontier. Otherwise a read refuses span_pending and a
closure refuses invalid_effect. A pair whose opening is later than frontier is absent
even if it uses the same old statement. The existing whole-result buffer ensures no
earlier entry escapes on refusal. Closed and retired spans use ended_content_id as
frontier, previews use through. The seed has no runtime frontier and attaches no pair.
Ordinary history continues to use Spec 05's current pair view. [MARK-3, MARK-4, PAIR-4,
READ-5]

**STATE-MARK-READ.** marks returns the tagged result in TYPES-READ. Resolve the largest
task local ID, then its unique task_prompt child, for current_task. Follow the single
open path in parent order for open_scopes. Convert open/end content IDs to their
SourceBlockIds. next_span_id is checked unsigned MAX(local_id)+1, and counter exhaustion
refuses counter_exhausted, never zero or a reservation. An adopted store must already
satisfy the path and source invariants. A missing prompt or impossible path is
structural failure, not an empty successful result. [MARK-1, MARK-6, SEAM-3]

span_preview requires an open span and a held through event at or after its open.
SourcePosition.turn must equal that event's stored turn, including null. Unknown or
earlier through, a terminal span, or inconsistent identity refuses invalid_selector. Run
STATE-MARK-MEMBERS and STATE-MARK-PAIRS at that frontier, return the tagged span_preview
result, and write no mark or member. Closed span reads use only immutable span_member
rows, with their fixed pair frontier. The load/save image carries all span, prefix and
member rows unchanged. Reason: review needs a view of current work without falsely
closing the work it reviews. [MARK-3, MARK-5, MARK-6, PLAN-2, PERSIST-1]

| Old reader or ask | Serving replacement or retirement |
| --- | --- |
| identity_of, Store::identity, ask_identity_within | system_prompt selector, span query above |
| recall, ask_recall and ask_recall_within | history selector for noun retrieval, explicit conversation selector below for the retained raw conversation consumer |
| shape | SELECT run,kind,COUNT(*) FROM content_block WHERE source_kind='event' GROUP BY run,kind ORDER BY MIN(id) |
| held | SELECT COUNT(*) FROM content_block |
| position_of and Store::position | SELECT run,sequence,block,turn FROM content_block WHERE source_kind='event' ORDER BY id DESC LIMIT 1 |
| replay | Retired in serving, unsupported_operation locally, diagnostic retains it |
| grants and ask_grants | Retained as a typed control, answered by the member room's surface(), not by SQLite |
| restored | opened result supplies loaded artifact facts, no second restore ask |
| snapshot and acknowledge | New persistence continuation on the same channel |
| Store::index_election and serving schema slot | Retired, fixed schema and indexes |
| Store::adopt and judge_image | Verify new schema and seeded span on scratch connection, adopt atomically |
| Store::land | Replaced in serving by ServingStore::file and its atomic receipt, retained in diagnostic |
| Store::retire_and_index | Diagnostic preload only, no serving caller or matching control |
| Store::schema | Serving catalog inspection checks the fixed schema, never accepts caller DDL, legacy schema text remains diagnostic |

Conversation requires last_turns, a positive DecimalU64. Select the newest N distinct
(run,turn) pairs by their greatest content ID, then all message.user and
message.assistant blocks belonging to those turns in ascending content ID order. No turn
is confused with the same turn ordinal in another run. Null-turn rows are excluded. An
absent or zero last_turns refuses invalid_selector. It exposes Text holdings only. It
does not pretend to reconstruct omitted tool traffic. The harness's old implicit
full-tail boot recall is retired in this unit. Boot seats the system prompt, and later
retrieval places selected history. A later retained caller of ask_recall must use the
explicit conversation selector and accept that scope. No caller still reads event/field.
Reason: a new writer cannot leave consumers querying retired tables. [PERSIST-6,
STORE-8, READ-1]

**STATE-PAIR-READ.** Content selectors produce QueryResult::Entries. Attach
pairs:Vec<ReadPair> to each returned statement using Spec 05's PAIR-READ query, ordered
by opened_event. No pair is invented for a statement with none. open_pairs produces
QueryResult::OpenPairs through the fixed partial-index query in that same clause. Its
order is opening content ID across the held chain. A boot repair uses this read and
ordinary file_pair, never an adoption-time update. Span, system_prompt and span_preview
instead use STATE-MARK-PAIRS and its fixed frontier. [READ-1, READ-4, PAIR-1, PAIR-3,
PERSIST-6]

Conversation and span text queries select statement rows. Pair returns are attached
whole through ReadPair. Shape, held and position continue to include both content types,
so a pair terminal can be the image's captured SourcePosition. This content position is
distinct from HARNESS-CAPTURE's local Recorded trace watermark. Untapped recall and
trace observations create no content row or SourcePosition. Paging bounds the complete
encoded tagged result with all attachments. It never drops an over-large pair.
[STORE-5, READ-5, PAIR-4, PERSIST-4]

**STATE-OBSERVATION-READ.** Only the observations selector reads these tables. It
carries {run:String,after:DecimalU64,through:Option<DecimalU64>}. run must be a valid
trace RunRef already in observation_run. A missing run refuses not_found. At read
acceptance, take MAX(operation_id) for that run, using zero if it is empty. Null through
selects that maximum. A supplied through greater than that maximum or after greater than
through refuses invalid_selector. This query returns the strict observations result
defined by TYPES-OBSERVATION-READ, in increasing operation_id order. The observation of
this read is filed afterward, so it cannot appear in its own result. No clock comparison
or lexical run ordering chooses this frontier. [SEAM-3, COST-2, R20-19]

```sql
SELECT operation_id, request_id, operation, source_run, source_sequence,
       source_block, observation
FROM operation_observation
WHERE run = :run AND operation_id > :after AND operation_id <= :through
ORDER BY operation_id
```

Read member_observation and the rows inside one read transaction. Reconstruct each
StoredOperationObservation with its source identity and validate its JSON against the
indexed columns and its run's recorded instrument version. Historical /1 and /2 retain
their original definitions. A stored mismatch is component failure, not an empty
success. The normal bounded-result writer, immutable paging, release and
result_too_large rules apply. Other selectors, held counts, shape, SourcePosition,
marks, plans and content grants exclude these tables. Querying observations is not
permission to turn them into content units or place them automatically. The caller owns
any separately requested presentation and must keep the typed query identity. [STORE-3,
SEAM-3, MARK-6, COST-2, R20-19]

**STATE-PAGES.** One immutable QueryResult at most is held per serving channel.
Serialize selected rows directly into a bounded RAM writer, without first copying the
whole row set into a second Vec. The declared reads.max-result-bytes bounds the complete
UTF-8 JSON result. Before growing the buffer past that value, abort the read, discard
its buffer and return result_too_large. No page or truncated result escapes. On success,
end the read transaction and page the immutable buffer, so later filing cannot change
pages. A second read before release refuses read_busy. Slice at most 524288 bytes per
page, base64-encoded in ReadPage. A slice may split UTF-8 because no slice is exposed as
content. Reassemble the complete result, validate it, then expose entries whole. This is
wire framing, not document chunking. The harness checks total_bytes against the same
bound before allocating its reassembly buffer. Record result byte count, copy high-water
memory and any result_too_large refusal. Measure the limit with the largest qualified
holdings, concurrent filing, both endpoint buffers and capture reserve under the
declared tuple. Freeze it before held-out qualification, and leave its number open here.
The qualification must also show that a complete result at that limit fits the existing
enclosing read deadline under the declared load. A byte limit alone proves no timing
claim. A tuple that misses the deadline is not qualified by shrinking a result silently.
[SEAM-3, CLASS-8, COST-4] The refusal is an explicit query outcome, never an empty
history. A consumer may issue a narrower declared query, but cannot silently omit
entries from this one. [SEAM-3, READ-5, STORE-1, COST-2]

Each ReadPage has snapshot_id, offset, total_bytes, octets, done. Offsets/counts are
DecimalU64. Snapshot ID is a checked run-local counter. next requires exact next offset
and the owning run. release destroys the buffer. EOF, halt and run end release it too. A
final page does not implicitly release it. Read errors are unsupported_operation,
invalid_selector, wrong_seed, not_found, unknown_span, span_open, span_pending,
partial_pair, counter_exhausted, read_busy, unknown_snapshot, wrong_offset,
result_too_large, result_unavailable. Read refusals change no content holdings. Their
operation metadata is retained under STATE-OBSERVE. A query allocation failure reports
result_unavailable when reportable. Engine loss or an unparseable result is component
failure. [SEAM-3, FLOW-4, COST-2]

Prepare the first complete bounded page before entering Held. Any initial read refusal,
including an allocation or first-page encoding refusal, discards the buffer and leaves
no cursor to release. A later valid next refusal preserves the already acquired cursor
until release or channel close. Reason: the caller can release only an identity it
received, and a failed read cannot leak a hidden snapshot. The first-page failure
fixture inspects that no buffer remains and that a following read is not read_busy.
Removing that cleanup must fail it. [SEAM-3, STORE-1, COST-2]

| Read state | Request | Action and next state |
| --- | --- | --- |
| Idle | read(selector) | Snapshot result, answer first page, Held |
| Idle | Result exceeds declared byte limit | Refuse result_too_large, discard partial buffer, Idle |
| Idle | Other valid read refusal before first page | Refuse named code, discard buffer, Idle with no cursor |
| Idle | next or release | Refuse unknown_snapshot |
| Held | next(id,exact offset) | Answer next page, advance offset |
| Held | read | Refuse read_busy, unchanged |
| Held | wrong id or offset | Refuse named code, unchanged |
| Held | release(matching id) | released, Idle |
| Any | EOF, halt, run end | Release buffer, Closed |

**STATE-CONVERSATION.** conversation selects complete message statement blocks from the
last positive last_turns distinct (run, turn) pairs in insertion order. It excludes null
turns and bookkeeping, and returns every binding and whole pair for each selected block.
The literal selection query is below. Pagination freezes the result once, like every
other read. [READ-1, STORE-3, STORE-5]

```sql
WITH chosen AS (
  SELECT run, turn, MAX(id) AS last_id FROM content_block
  WHERE source_kind = 'event' AND content_type = 'statement'
    AND turn IS NOT NULL AND kind IN ('message.user','message.assistant')
  GROUP BY run, turn ORDER BY last_id DESC LIMIT :last_turns
)
SELECT b.id FROM content_block b JOIN chosen c
  ON b.run = c.run AND b.turn = c.turn
WHERE b.content_type = 'statement'
  AND b.kind IN ('message.user','message.assistant')
ORDER BY b.id
```

Convert DecimalU64 last_turns to SQLite's positive signed LIMIT after clamping to the
number of held distinct turns. A value above i64::MAX selects all turns, not a negative
LIMIT. No run identifier is used as a clock. [STORE-5, READ-1]

## 3. Member mode and channel, destinations sections 2 and 4

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**STATE-MODE.** This explicitly reverses the 2026-10-02 no-vector-flag ruling for the
state member's launch vector, under the diagnostic/serving separation of R8. The
admin-owned --mode selects custody, never a loop strategy. The admin-owned launch vector
explicitly selects serving, seed, or diagnostic. Replace implicit preload-based
inference with `--mode=<value>` supplied only by admin. Retain fd3 first door and fd9
run lock. Serving requires read-only fd4 with the raw SQLite dump. Seed refuses fd4 and
any preload door. Diagnostic retains its preload arguments and old schema on a separate
connection. A mode/protocol mismatch closes before any mutation. Old one-file images and
old catalogs refuse serving load as unsupported_schema. Recreate old agents under the
no-old-layout rule, never silently migrate them. [PERSIST-3, PERSIST-5, SCOPE-1, R8]

**STATE-CHANNEL.** Use state-flow/1 on fd3. The same parser receives typed controls from
the types and persistence amendments. Protocol selection never depends on untrusted
first-frame shape. A file may be accepted only after opened. The table is complete for
ordinary traffic, with persistence continuation states in STATE-PERSIST-STATES below.
[SEAM-1, SEAM-2]

| State | Event | Action and next state |
| --- | --- | --- |
| AwaitOpen | Valid open and verified store | opened with starting facts, Ready |
| AwaitOpen | Unsupported schema or refused image | correlated refused, close |
| AwaitOpen | Any other request or response | Close without answer or mutation |
| Ready | file or file_pair | Retain request identity and enter Committing |
| Committing | Validation or transaction completes | Finish observation and queue filed or refused, SendingAnswer with next Ready or Closed |
| Ready | control | Retain request identity and enter Dispatching |
| Dispatching | Read or small control completes | Finish observation and queue control answer, SendingAnswer with next Ready |
| Dispatching | snapshot accepted | Enter Capturing in STATE-PERSIST-STATES |
| Ready | second open | protocol failure, close |
| Committing | Request bytes arrive | Do not read, harness may send nothing else |
| SendingAnswer | Partial write or WouldBlock | Retain exact bytes and offset |
| SendingAnswer | Bytes readable | Do not read until answer ends, coalesced next request then parsed in Ready |
| SendingAnswer | Full filed/control answer sent | Ready or continuation state |
| SendingAnswer | Full filing refusal sent | Close |
| Any | EOF, over-bound frame, malformed identity, terminal I/O | Close, no recovery save |

Keep incoming FRAME_BOUND at 8 MiB and control ANSWER_BOUND at 1 MiB including frame
wrappers. Refuse a complete oversized control result as answer_too_large. Large reads
use STATE-PAGES. No partial filing is admitted. [FLOW-4, SEAM-2, SEAM-3]

**STATE-OBSERVE.** The serving answer_frame replacement brackets every request and
persistence continuation with Spec 09's STORE-OBSERVATION. Capture began_ns before
operation hint parsing and allocate its operation ID once before validation. Snapshot
uses that same wall boundary. Its CPU/disk samples separately bracket capture work under
SNAPSHOT-SAMPLING, inside the operation interval. Finish the content transaction or read
and encode the operation payload before closing the observation interval. Then
atomically insert the observation and its source identity into operation_observation in
a separate RAM bookkeeping transaction. For successful open, insert observation_run and
the first observation together after all adoption and identity checks. Only then encode
the wrapper and enter SendingAnswer. [COST-2, COST-5, SEAM-2, R20-19]

The metadata insert is not a received operation, generates no nested observation and
runs no classifier. It writes no content table and has no external filing port. Its cost
and wrapper encoding remain in the harness's full interval, outside the sampled
operation interval. A metadata insertion failure closes the channel before success is
sent. A prior content commit remains uncertain, so the harness halts and never resends
it. Missing optional counters instead retain explicit unavailable values. The member
samples OS CPU/disk facts only during snapshot, not ordinary operations. The open
identity is sampled once. All writes here are to the RAM connection. [FLOW-4, FLOW-5,
STORE-1, COST-2, R20-4, R20-19]

The response carries the same observation stored beside its operation. Retain its
encoded bytes and ID through partial writes. Do not insert again or resample on retry. A
source-matched refusal also records metadata before its answer. Uncorrelatable malformed
input has no invented record. A rejected open without a validated run has no serving
metadata account and cannot claim adoption success. Diagnostic and seed codecs remain
separate. [FLOW-3, SEAM-2, SCOPE-1, R20-19]

A dump includes metadata committed before capture began. The snapshot operation's own
observation, its acknowledgement and release are later RAM records and are absent from
that captured image. A later capture may include them. This is an explicit metadata
cutoff, not a recursive rewrite of a hashed image. CaptureHeld permits only this
internal bookkeeping for allowed continuations, never content filing. Observation
metadata does not advance SourcePosition. On reload, old run rows remain historical and
the new open uses a new run identity with operation_id one. New runs use
state-observation/2. Adoption validates historical /1 and /2 identities without
rewriting them, and explicit observations reads return each run's original identity.
This does not admit a current opened response using /1. [PERSIST-1, PERSIST-3,
PERSIST-4, COST-2, COST-5, R20-19]

STATE-PAGES reserves space for the observation and wrapper as frame accounting.
ReadPage.octets holds only its typed QueryResult. Other queries never receive stored
observations, even though their responses carry a new harness-bound observation. Only an
observations query returns this metadata as query data. The harness carries response
metadata to state.store.measurement and never treats it as selected content. [SEAM-3,
TAP-3, COST-2, R20-19]

| Metadata state | Event | Result |
| --- | --- | --- |
| Operation completed | Payload encoded, clock/identity valid | Build one observation, PersistingMetadata |
| PersistingMetadata | RAM insert committed | Encode wrapper once, SendingAnswer |
| PersistingMetadata | Insert or required identity fails | Close channel, retain uncertain content outcome, no recursive refusal |
| SendingAnswer | Partial write | Retain bytes and existing metadata row |
| SendingAnswer | Complete write | Existing protocol continuation, no second row |
| Read snapshot held | Later operation metadata | Immutable query bytes unchanged |
| Captured image held | Snapshot/acknowledge/release observation | Record only in live RAM, never patch captured image |

The actual server fixture injects samples into the real in-memory store. Exercise open,
file, pair, all controls, refusal, rollback, pagination and interrupted sends. Query the
metadata explicitly and compare with the carried record, then prove ordinary content
queries and counts exclude it. Inject metadata-commit failure after a content commit and
require channel closure without resend. Remove the separate-table rule,
commit-before-answer guard, run key, fixed read frontier or single-insert guard in
separate mutations. Each must fail its fixture. Snapshot/reload must preserve exactly
its captured metadata cutoff. This pass proves accounting, not zero disk wait.
[FLOW-4, SEAM-2, SEAM-3, COST-5, FALS-3, R20-19]

The actual snapshot fixture records ordered hooks at the operation timestamp, hint
parse, validation, before counters, serialization, room-file completion, after counters
and payload encoding. Exercise success, preflight refusal, serialization failure,
recoverable room-write failure and counter unavailability. Partial sends must reuse the
completed observation without sampling again. Require the Spec 09 order, with no
sampling on ordinary requests or a preflight refusal. The full wall interval includes
both counter reads. Remove each ordering or sampling guard separately and require a
behavioral assertion to fail. Adopt an image with historical /1 metadata, open a new /2
run, and read the old records without relabeling them. Removing the
current-versus-historical version check must fail its fixture. This proves timing
boundaries and identity, not counter accuracy or absence of disk wait. [COST-1, COST-2,
COST-4, COST-5, PERSIST-1, FALS-3]

## 4. Seed, adoption, and capture, destinations sections 2 and 3

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**STATE-SEED.** Seed mode accepts exactly one SeedRequest, creates the schema, and
inserts one complete STE100 prompt Text block with supplied certified bindings and span
0 revision 1 in one transaction. Seed source has ordinal zero, all event fields null,
null context/effect, and admission seeded. The span is closed and has one complete
prefix reference and one identical finalized member. It accepts no live file or
arbitrary SQL. Validate the certificate's prompt digest, scheme, complete PROMPT-3 duty
list and all binding shapes. Use TYPES-SEED's supplied scheme bytes and expected checker
digest with Spec 02's validator. Require their scheme identity to equal
request.scheme_id and certificate.scheme_id. No root path, model or serving
configuration is opened by this mode. Semantic conformance evidence is the classifier
amendment's certificate, not a new classifier in this member. Reason: seed is the sole
pre-agent write. [PERSIST-5, PROMPT-1, PROMPT-2, PROMPT-3]

**STATE-ADOPT.** Load the raw fd4 image into a scratch in-memory connection. Require
exact state-content/1 catalog, quick_check success, no foreign_key_check rows, one
store_meta row, one complete system_prompt span and valid bindings. Compare that row's
scheme_id to open.scheme_id and its seed_id to open.start.seed_id. Compare the last
committed source position to open.start.position. Refuse scheme_mismatch on a scheme
difference and open_failed on any other structural difference. For every saved recall
pair, require its held binding to have exactly one noun, no governing condition and
subject bytes equal to that noun, as in STATE-PAIR-FILE. A mismatch refuses open_failed
before adoption. It is not a live invalid_pair filing refusal and changes no saved row.
Echo the verified scheme and StartIdentity in opened. Adopt once only after every check.
For marks, validate consecutive local IDs, revision 1, permitted parent/kind pairs, one
open path, one prompt child per task, source and prefix references, all UTF-8 and
binding boundaries, and matching source contexts/effects. Recompute every runtime
terminal span's canonical members at its recorded end and compare them with span_member
byte for byte and ordinal for ordinal. Check its fixed pair frontier. Open spans have no
finalized members. The seed is the sole source-free closed span. Refuse a structurally
inconsistent image rather than repairing it. [MARK-1, MARK-3, MARK-5, PERSIST-6] No old
Election argument or index_election runs. A failure leaves no serving store and refuses
open. Pass means schema and structural integrity of those bytes, not semantic truth or
authorization of an operator's prior choices. The evidence is the verified image A and
the measured catalog/check results. [PERSIST-3, PERSIST-6, STORE-3]

**STATE-CAPTURE.** After the harness barrier, serialize the current in-memory SQLite
connection to a complete RAM image, as Store::image does today. Serving remains paused
through publication, so there is no concurrent mutator or new dump worker. Write that
immutable image to a member-room part and return its name and size. No disk write is a
turn store operation. The capture copy adds approximately one whole image plus the
serialization buffer to peak memory, measured under the declared tuple. Retain the
existing 1 GiB image bound. Over-bound capture returns image_too_large, not a partial
image or silence. Admin alone computes A and B and publishes. [STORE-1, PERSIST-1
through PERSIST-4, COST-2, R1]

**STATE-NO-SAVE.** A failed flow cannot request capture, finish an unconfirmed
publication, or adopt an orphan from the member's room. Delete parts on exit. The
three-party contract defines confirmation and crash boundaries. The member never writes
root publication indexes or a configuration file. [FLOW-5, R3]

## 5. Enforcement, destination section 5

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**STATE-FILING-INSTRUMENTS.** Put transaction instruments in
engine/serving_sqlite/tests.rs, beside the serving engine they exercise. The fixture
calls the production ServingStore::file and file_pair methods on a real in-memory SQLite
connection. Test-only access to that private connection supplies before/after reads of
every affected table. It creates no serving inspection ask, exported test store,
production feature or writable file door. Reason: receipt and rollback assertions must
observe the transaction the serving path executes. [STORE-1, STORE-3, SEAM-2, FLOW-4]

Under cfg(test), each serving-engine instance may hold one FilingTestHooks value. It has
checkpoint(point, transaction) -> Result<(), InjectedWriteFailure> and
committed(connection) methods. The arguments borrow the real rusqlite Transaction or
Connection. Checkpoints are after_source, after_binding(binding_ordinal),
after_noun(binding_ordinal,noun_ordinal), after_effect and before_commit. Plain
statements still visit after_effect, with no effect rows changed. Pair writes visit
after_source, after_effect and before_commit. A checkpoint error follows the production
write_failed rollback path. committed runs only after SQLite commit succeeds and before
FileCommit is constructed. It may observe but cannot return a new refusal after commit.
There is no process-global hook or environment variable, and these members and calls do
not compile into a normal binary. [STORE-3, SEAM-2, FLOW-4]

For every checkpoint, inject failure and require identical prior rows, no receipt and no
surviving source, index or effect. Also cause a real deferred foreign-key violation
inside before_commit in the test transaction. SQLite commit must then fail, committed
must not run and the same rollback assertions must hold. Successful cases require
committed to observe all rows and exact source identities before the server constructs
its filed response. Removing the transaction, ignoring a checkpoint error, constructing
success before commit or converting commit failure to FileCommit must break the
corresponding test. The unit fixture may inspect private engine state, but no harness
test links this crate for that purpose. [STORE-3 through STORE-5, SEAM-2]

**STATE-CIRCUIT-INSTRUMENT.** The harness's tests/state_flow.rs spawns the normal
weaver-state executable, with inherited first-door fd3, read-only image fd4 and run-lock
fd9. It uses the real recorder, coordinator and state codecs, and the scripted
Ste100Port only for language results. A synthetic state-start fixture supplies the
complete catalog and seed span. Its provenance identifies it as test material, never a
qualified production seed. The fixture sends no hidden read or mutation and uses the
ordinary typed queries to inspect accepted content. Resolve the executable as
current_exe().parent().parent().join("weaver-state"), beside the test profile's deps
directory. Descriptor setup follows this crate's tests/run_lock.rs process fixture. A
missing binary fails the fixture with the build command, never skips it. [FLOW-7,
SEAM-1, SEAM-2]

The document act's code successor owes these invocations, from the repository root with
the same toolchain, target and Cargo profile:

```sh
cargo test -p weaver-state --locked --features sqlite
cargo build -p weaver-state --locked --features sqlite --bin weaver-state
cargo test -p weaver-harness --locked --test state_flow
```

A test-owned stream proxy may hold, fragment, corrupt or discard a real response between
the member and harness. It keeps the captured response bytes as evidence. This is how
the circuit withholds a committed receipt without adding a test command to the member.
The harness must neither acknowledge a held answer nor resend the request, and a
corrupted identity must never become filed. A stopped member proves loss handling.
Deterministic transaction failures use the in-crate hooks above, not a claim that the
spawned normal binary contains those hooks. Circuit cleanup kills and waits for its
child and removes its private fixture directory. The harness manifest gains no
weaver-state dependency of any kind. [FLOW-4, FLOW-5, SEAM-2, SCOPE-1]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| STATE-CONNECTION | Seed and adopted-image connection fixtures read settings and build facts before use, with rejected fact variants | Remove the initializer, accept TEMP_STORE=0 or unknown build facts, or expose an adopted connection before rechecking |
| STATE-OBSERVATION-SCHEMA/READ/OBSERVE | Exact metadata JSON, real response writes, explicit query, capture metadata-cutoff, ordered snapshot-sampling and historical-version fixtures | Mix metadata with content, reobserve an insert, omit a required row, include the read itself, send success after metadata failure, reorder sampling or relabel old evidence |
| STATE-SCHEMA | Execute literal DDL, inspect catalog, EXPLAIN noun and verb queries | Remove index, source CHECK, or FK |
| STATE-FILE | STATE-FILING-INSTRUMENTS and shared malformed corpus | Emit receipt before commit or accept partial noun rows |
| STATE-PLAN-SCHEMA, STATE-PLAN-READ | Literal DDL, ordered current/historical reads and reload, exact operator digest and null/non-null last-transition turn | Allow two unfinished plans/active steps, renumber IDs, omit references, hash the snapshot or substitute another source/digest/turn |
| STATE-PLAN-FILE | Every effect guard and atomic source/plan/mark transaction | Skip expected status, close without category-required review evidence, preserve changed evidence or commit one half |
| STATE-PLAN-FLAGS | Literal target/count queries, stale source/revision checks, empty plans and maximum unsigned IDs | Count nouns, case-fold addresses, accept an overflowing/noncanonical ID, use an old target set or change a held sample after filing |
| STATE-MARK-FILE | Atomic source/effect fixtures for every operation row | Commit the source without the mark, skip context equality, permit a crossed scope or edit terminal membership |
| STATE-MARK-MEMBERS | Literal recursive query with nested prefixes, ignored events and closure outcomes | Exclude the shared opening/closing anchor, include bookkeeping or reorder by trace run |
| STATE-MARK-PAIRS | Fixed-frontier fixtures with partial, open and later pairs | Omit the opening cutoff or accept a return beyond the frontier |
| STATE-MARK-READ | Tagged marks/preview results and dump/reload fixtures | Write on preview, accept an open span as closed, omit allocation exhaustion or alter membership on load |
| STATE-PAIR-FILE | Pair lifecycle transactions, compound/conditional recall openings, matching adoption fixtures and failure injection | Accept a second terminal, weaken the exact one-noun/unconditional shape, leave an orphan source row or emit receipt before commit |
| STATE-PAIR-READ | History/open-pair queries with complete raw returns | Lose an attachment, parse a pair as Text or omit terminals from position |
| STATE-READ | Prepared-query fixtures across two runs and compound nouns | Query old event table, reorder sessions, mutate on read, or hide seed |
| STATE-SUBJECTS | Literal SQL with compound nouns, two runs, bookkeeping and an unmatched binding's pair | Remove source-kind restriction, order by run, return only matched bindings or lose the pair |
| STATE-STATEMENT | Exact nonzero binding ordinal, absent source and absent binding fixtures | Ignore ordinal, renumber the returned binding or report absence as empty success |
| Reader retirement | Source search plus boot/recall/shape/position integration tests | Leave a serving query against event or field |
| STATE-PAGES | Result limit at and one byte past, concurrent filing, offset and EOF cases | Remove bounded writer, truncate a block, mutate snapshot, or leak second cursor |
| STATE-MODE | Process launch matrix and preload probes | Infer mode from frame or allow serving preload |
| STATE-CHANNEL | Every row, partial writes, exact frame limits | Accept second open, over-bound frame, or premature second exchange |
| STATE-SEED | Seed-mode process test, TYPES-SEED corpus and live-writer refusal | Seed an existing store, ignore a supplied scheme/checker mismatch or admit prompt without end mark |
| STATE-ADOPT | Wrong catalog/hash/span/foreign-key images and saved compound/conditional recall pairs | Adopt before every check, weaken recall shape to noun membership, use a live filing refusal for an invalid image or accept old one-file format |
| STATE-CAPTURE | Member-room process fixture with size and disk failures | Publish partial image or mutate during capture |
| STATE-CIRCUIT-INSTRUMENT | Spawned member, real recorder and response proxy | Skip a missing binary, accept a held/corrupted receipt, resend uncertain filing or query a hidden test door |
| STATE-NO-SAVE | Kill at each capture/confirmation boundary | Recover an unconfirmed image as latest |

## 6. Capture and seed states, destinations sections 2 and 4

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

**STATE-PERSIST-STATES.** Snapshot continuations hold the same outer request ID. After
finished, a fresh release_capture request releases a successful completed capture. A
timeout cleanup instead uses abort with the original request and capture IDs, including
in FinishedHeld. It consumes no fresh exchange ID. All snapshot answers obey
ANSWER_BOUND. Reason: no second writer or unowned room image may appear during
persistence, and a timed-out capture must release its holdings before retry or filing.
[SEAM-2, PERSIST-1 through PERSIST-5]

| State | Event | Action and next state |
| --- | --- | --- |
| Dispatching, no capture | Valid snapshot | Retain capture ID and enter Capturing |
| Capturing | Complete image within size bound | Queue snapshot answer, then CaptureHeld after full send |
| Capturing | I/O failure or image too large | control_refused, delete part, retain refused capture identity for correlated cleanup, Ready |
| CaptureHeld | acknowledge with matching capture and syntactically valid A | Retain admin's A as attribution, finish image, send finished, FinishedHeld |
| CaptureHeld or FinishedHeld | abort with original request and matching capture ID | Delete retained part or finished room file, send aborted, Ready |
| Ready with just-refused capture | abort with that original request and capture ID | No room file remains, send aborted, clear retained identity, Ready |
| Ready with just-refused capture | Next fresh request | Clear refused capture identity, use STATE-CHANNEL |
| CaptureHeld or FinishedHeld | file or file_pair | Send filing refused with wrong_phase, then close |
| CaptureHeld | Any other control | invalid_capture_state, retain capture |
| FinishedHeld | release_capture | Delete finished room file, capture_released, Ready |
| FinishedHeld | Any read, snapshot, wrong abort, or wrong release | control_refused invalid_capture_state, remain FinishedHeld |
| Any capture state | EOF or terminal failure | Delete retained room files, close |

Partial answer writes retain their bytes and continuation state. The member reads no
continuation until the preceding answer is fully sent. An unexpected second open always
closes, including during a capture. A malformed identity or frame closes rather than
fabricating a correlated refusal. The member never computes A and cannot claim that it
compared A with its bytes. Admin owns that hash check. A malformed A is a malformed
control, not a successful acknowledgement. [SEAM-2, FLOW-4, R1]

The one retained refused capture identity covers a race: the harness may time out and
send abort before receiving a completed capture refusal. It permits only that abort,
not another acknowledge, snapshot or old exchange. Successful aborted clears the held
capture, and the next fresh request uses the ID after the original snapshot exchange.
A delete failure is component failure, never aborted or Ready. The member finishes any
queued snapshot, finished or control_refused answer before reading abort. The harness
discards that late operation result under PERSIST-CLEANUP, then waits for aborted.
[PERSIST-4, SEAM-2, FLOW-4]

| Seed state | Event | Action and next state |
| --- | --- | --- |
| AwaitSeed | Valid single SeedRequest | Build schema and prompt transaction, Creating |
| Creating | Validation or insertion fails | TYPES-SEED refusal, rollback any insertion, no image, Closing |
| Creating | Transaction committed | Serialize, write completed room image, answer created, Held |
| Creating | Capture fails | refused snapshot_failed, delete image, Closing |
| Held | EOF after admin copy, or another request | Delete room image, close |
| Any | Component failure | No publication authority, clean parts and exit |

The seed image follows the same 1 GiB bound. The SeedCreated result from the admin
amendment is defined in weaver-types::state_flow. Admin indexes it only after fully
validating and copying it. No serving SavePointCommit is needed before an agent exists.
Schema verification includes the SQLite-created sqlite_sequence table and its normal
AUTOINCREMENT semantics. [PERSIST-3, PERSIST-5]

For reads, add Idle/read failure -> control_refused result_unavailable with no cursor.
An engine failure closes the channel. Held/next after done refuses wrong_offset and
keeps the cursor until release. Request offsets never rewind. A caller cannot infer
partial content from the base64 pages. [SEAM-3, FLOW-4]

| Claim | Instrument | Protective change that must fail |
| --- | --- | --- |
| STATE-PERSIST-STATES | Every seed/capture cell, timeout abort in CaptureHeld and FinishedHeld, late refusal race, partial answers and EOF | Finish before A confirmation, refuse a correlated cleanup, send aborted before deletion, consume a fresh ID for abort or retain an orphan as loadable |

## 7. Retired storage representations, destinations sections 2, 3 and 6

This replacement applies at the [foundation cutover][cutover]. Until then, the previous
serving contract remains applicable.

| Baseline element | New serving representation | Diagnostic fate |
| --- | --- | --- |
| event | content_block attribution and source order | Retained |
| field | Raw source block plus statement_binding/binding_noun | Retained |
| message | No typed duplicate, canonical Text block is content | Retained |
| part | No decomposition table, binding spans address content | Retained |
| measurement | Legacy payload table retired, new operation metadata uses STATE-OBSERVATION-SCHEMA | Retained |
| series | Excluded with measurement payload | Retained |
| stamp line and check line | binding.json A/B, schema_id, confirmation and SourcePosition | Legacy format retained only for diagnostic |
| schema digest in stamp | Exact state-content/1 catalog check on image bytes | Legacy check retained |
| digest.save-point room name | Capture name below, hash computed by admin | Legacy name retained |
| dated one-file publication name | Ordinal-B directory plus indexed binding | No serving adoption of legacy image |
| fixtures/save-points shared corpus | Add fixtures/state-start-v1 three-file positive/negative corpus in weaver-types tests | Old corpus remains for diagnostic tests |

Serving room names are capture-<capture_id>.sqlite.part, then
capture-<capture_id>.sqlite after acknowledgement. The run lock excludes a second
member, startup removes unconfirmed remnants, and create_new refuses collisions. Seed
uses seed-<seed_id>.sqlite after its successful transaction. Neither name requires the
member to compute A. Admin accepts exactly this grammar within the judged room
descriptor, never path traversal. [PERSIST-2, PERSIST-3, R1, R8]

Table retirement is tested by querying sqlite_master in serving and diagnostic fixtures
separately. The old stamp/name fixture is refused by the serving adopter, and a
three-file artifact with a false position must fail its position comparison. This is
part of STATE-ADOPT and ADMIN-RETIRE, not a silent migration. [PERSIST-3]

STATE-PAGES enforcement includes a table-driven refusal case for every ReadRefusalCode:
unsupported_operation, invalid_selector, wrong_seed, not_found, unknown_span, span_open,
span_pending, partial_pair, counter_exhausted, read_busy, unknown_snapshot,
wrong_offset, result_too_large, and result_unavailable. For each, compare all held rows
before/after excluding the new operation metadata, remove the named validation check,
and require the test to fail. The result_unavailable case injects allocation failure at
the real snapshot-copy boundary. Engine failure separately proves channel closure.
[SEAM-3, FLOW-4]

## 8. Open elections and dependencies

Production template and prompt certification are owed to the classifier amendment. The
prompt itself is owed separately. The mark catalog and read forms above are integrated,
and the plan catalog and effect arm are defined above. Spec 05 defines the pair
transitions, joins and R4 closure now consumed here. Retrieval injection remains Spec
08's consumer of these reads. An incomplete catalog cannot pass STATE-ADOPT or the
deployment gate. Read-result RAM cost, image-copy cost and maximum qualified store size
remain measured. The R20 rulings are applied. Approval and code evidence remain owed.

[cutover]:
state-management-foundation-Spec.md#1-applicability-and-the-serving-cutover

[sqlite-temp]: https://www.sqlite.org/pragma.html#pragma_temp_store
