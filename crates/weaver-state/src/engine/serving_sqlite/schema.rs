//! The literal state-content/1 catalog from STATE-SCHEMA,
//! STATE-OBSERVATION-SCHEMA and STATE-PLAN-SCHEMA.

pub(super) const DDL: [&str; 27] = [
    r#"CREATE TABLE store_meta (
  singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
  schema_id TEXT NOT NULL CHECK(schema_id = 'state-content/1'),
  seed_id TEXT NOT NULL UNIQUE,
  scheme_id TEXT NOT NULL
)"#,
    r#"CREATE TABLE content_block (
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
)"#,
    r#"CREATE TABLE statement_binding (
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
)"#,
    r#"CREATE TABLE binding_noun (
  binding_id INTEGER NOT NULL REFERENCES statement_binding(id),
  ordinal BLOB NOT NULL CHECK(length(ordinal) = 8),
  value TEXT NOT NULL,
  start BLOB NOT NULL CHECK(length(start) = 8),
  end BLOB NOT NULL CHECK(length(end) = 8),
  PRIMARY KEY(binding_id, ordinal)
)"#,
    r#"CREATE INDEX binding_verb ON statement_binding(verb COLLATE BINARY)"#,
    r#"CREATE INDEX binding_condition ON statement_binding(condition COLLATE BINARY)"#,
    r#"CREATE INDEX noun_value ON binding_noun(value COLLATE BINARY)"#,
    r#"CREATE TABLE pair_event (
  id INTEGER PRIMARY KEY REFERENCES content_block(id),
  record_type TEXT NOT NULL CHECK(record_type IN ('started','ended'))
)"#,
    r#"CREATE TABLE tool_pair (
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
)"#,
    r#"CREATE INDEX pair_statement ON tool_pair(statement_id)"#,
    r#"CREATE INDEX pair_open ON tool_pair(opened_event) WHERE fate = 'opened'"#,
    r#"CREATE TABLE span (
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
)"#,
    r#"CREATE UNIQUE INDEX span_open_root ON span((1))
WHERE parent_local_id IS NULL AND status = 'open'"#,
    r#"CREATE UNIQUE INDEX span_open_child ON span(parent_local_id, parent_revision)
WHERE parent_local_id IS NOT NULL AND status = 'open'"#,
    r#"CREATE UNIQUE INDEX span_task_prompt ON span(parent_local_id, parent_revision)
WHERE kind = 'task_prompt'"#,
    r#"CREATE INDEX span_parent ON span(parent_local_id, parent_revision, local_id)"#,
    r#"CREATE TABLE span_prefix (
  local_id BLOB NOT NULL, revision BLOB NOT NULL,
  ordinal BLOB NOT NULL CHECK(length(ordinal) = 8),
  block_id INTEGER NOT NULL REFERENCES content_block(id),
  start BLOB NOT NULL CHECK(length(start) = 8),
  end BLOB NOT NULL CHECK(length(end) = 8 AND end > start),
  step_query TEXT,
  PRIMARY KEY(local_id, revision, ordinal),
  FOREIGN KEY(local_id, revision) REFERENCES span(local_id, revision)
)"#,
    r#"CREATE TABLE span_member (
  local_id BLOB NOT NULL, revision BLOB NOT NULL,
  ordinal BLOB NOT NULL CHECK(length(ordinal) = 8),
  block_id INTEGER NOT NULL REFERENCES content_block(id),
  start BLOB NOT NULL CHECK(length(start) = 8),
  end BLOB NOT NULL CHECK(length(end) = 8 AND end > start),
  step_query TEXT,
  PRIMARY KEY(local_id, revision, ordinal),
  FOREIGN KEY(local_id, revision) REFERENCES span(local_id, revision)
)"#,
    r#"CREATE TABLE observation_run (
  run TEXT PRIMARY KEY,
  member_observation TEXT NOT NULL
)"#,
    r#"CREATE TABLE operation_observation (
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
)"#,
    r#"CREATE TABLE plan_revision (
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
)"#,
    r#"CREATE UNIQUE INDEX plan_one_unfinished ON plan_revision((1))
WHERE status IN ('authorized','locked','broken')"#,
    r#"CREATE TABLE plan_category (
  plan_id BLOB NOT NULL, revision BLOB NOT NULL,
  category_id TEXT NOT NULL,
  ordinal BLOB NOT NULL CHECK(length(ordinal) = 8),
  category TEXT NOT NULL,
  PRIMARY KEY(plan_id, revision, category_id),
  UNIQUE(plan_id, revision, ordinal),
  FOREIGN KEY(plan_id, revision) REFERENCES plan_revision(plan_id, revision)
)"#,
    r#"CREATE TABLE plan_group (
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
)"#,
    r#"CREATE TABLE plan_step (
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
)"#,
    r#"CREATE UNIQUE INDEX plan_one_active ON plan_step((1)) WHERE status = 'active'"#,
    r#"CREATE TABLE plan_group_step (
  plan_id BLOB NOT NULL, revision BLOB NOT NULL,
  group_id TEXT NOT NULL, step_id BLOB NOT NULL,
  PRIMARY KEY(plan_id, revision, step_id),
  FOREIGN KEY(plan_id, revision, group_id)
    REFERENCES plan_group(plan_id, revision, group_id),
  FOREIGN KEY(plan_id, revision, step_id)
    REFERENCES plan_step(plan_id, revision, step_id)
)"#,
];
