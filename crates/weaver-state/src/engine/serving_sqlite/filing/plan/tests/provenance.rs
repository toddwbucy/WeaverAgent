use super::*;
use crate::engine::serving_sqlite::{OpenFault, image};

mod internal_view;
mod summary;
mod work_bytes;

struct Fixture {
    store: ServingSqlite,
    start: StartIdentity,
    common: Common,
}

impl Fixture {
    fn new() -> Self {
        let request = image::tests::fixture();
        let seed = image::seed_image(&request).unwrap();
        let start = StartIdentity {
            seed_id: request.seed_id,
            artifact_id: "plan-provenance-fixture".into(),
            dump_sha256: digest(seed.as_bytes()),
            config_sha256: digest(b"plan fixture configuration"),
            parent_artifact_id: None,
            position: None,
        };
        let common = Common {
            session_id: "session".into(),
            run_id: "run".into(),
            request_id: 1.into(),
        };
        let store = ServingSqlite::adopt(
            seed.as_bytes(),
            &common,
            "state-content/1",
            &request.scheme_id,
            &start,
            ReadLimits {
                max_result_bytes: 1_000_000.into(),
                measurement_ref: "plan provenance fixture".into(),
            },
        )
        .unwrap();
        Self {
            store,
            start,
            common,
        }
    }

    fn file(&mut self, mut filing: StatementFile) {
        filing.scheme_id = self.store.context.scheme_id.clone();
        self.store.file(&filing).unwrap();
    }

    fn begin(&mut self, authorize: bool) {
        let operation = if authorize {
            PlanInputOperation::Authorize {
                task: 0.into(),
                categories: categories(false),
                groups: vec![],
                review: setup(),
            }
        } else {
            PlanInputOperation::Create {
                task: 0.into(),
                steps: vec![PlanInputStep {
                    id: None,
                    block: 1.into(),
                }],
                categories: categories(false),
                groups: vec![],
                review: setup(),
            }
        };
        let ctx = context(operation, None);
        self.file(input(1, 0, &ctx));
        if !authorize {
            self.file(input(1, 1, &ctx));
        }
        let task = TaskScopeRef {
            task: span(1),
            prompt: span(2),
        };
        let opening = if authorize {
            PlanEffect::Authorize {
                plan: plan(1),
                source: block(1, 0),
                task,
                review: setup(),
            }
        } else {
            PlanEffect::Lock {
                plan: plan(1),
                source: PlanLockSource::Operator { input: block(1, 0) },
                task,
                steps: vec![PlanStepInit {
                    step_id: step(1),
                    instruction: reference(1, 1),
                }],
                review: setup(),
            }
        };
        self.file(effect(2, opening));
    }

    fn complete(&mut self) {
        let (successor, _) = reads::step_successor(
            &self.store.connection,
            &self.store.context.seed_id,
            &plan(1),
            None,
            self.store.read_limits.max_result_bytes,
        )
        .unwrap();
        self.file(effect(
            3,
            PlanEffect::Activate {
                expected: plan(1),
                step_id: step(1),
                step_span: span(3),
                group_span: None,
                successor,
            },
        ));
        let work = work_ref(&self.store, 3);
        self.file(effect(
            4,
            PlanEffect::Close {
                expected: plan(1),
                step_id: step(1),
                review_call: None,
                work,
                outcome_span: span(4),
            },
        ));
    }

    fn replace(&mut self, changed_task: bool) -> SpanRef {
        self.replace_with_digest(changed_task, Digest::new("1".repeat(64)).unwrap())
    }

    fn replace_with_digest(&mut self, changed_task: bool, document_sha256: Digest) -> SpanRef {
        let (sequence, task_id, index) = if changed_task { (3, 3, 1) } else { (5, 5, 0) };
        let mut ctx = context(
            PlanInputOperation::Replace(PlanInputReplacement {
                task: changed_task.then(|| 0.into()),
                steps: vec![PlanInputStep {
                    id: None,
                    block: index.into(),
                }],
                categories: categories(false),
                groups: vec![],
                review: setup(),
                preserved_closures: vec![],
            }),
            Some(plan(1)),
        );
        ctx.document_sha256 = document_sha256;
        self.file(input(sequence, 0, &ctx));
        if changed_task {
            self.file(input(sequence, 1, &ctx));
        }
        let prompt = span(task_id + 1);
        self.file(effect(
            sequence + 1,
            PlanEffect::Replace {
                expected: plan(1),
                next: plan(2),
                source: block(sequence, 0),
                task: TaskScopeRef {
                    task: span(task_id),
                    prompt: prompt.clone(),
                },
                steps: vec![PlanStepInit {
                    step_id: step(2),
                    instruction: reference(sequence, index),
                }],
                review: setup(),
            },
        ));
        prompt
    }

    fn adopt(&self, bytes: &[u8]) -> Result<ServingSqlite, OpenFault> {
        let mut start = self.start.clone();
        start.dump_sha256 = digest(bytes);
        start.position = self.store.position().unwrap();
        ServingSqlite::adopt(
            bytes,
            &self.common,
            "state-content/1",
            &self.store.context.scheme_id,
            &start,
            self.store.read_limits.clone(),
        )
    }

    fn retarget_prompt(&self, prompt: &SpanRef, block_id: i64) -> Vec<u8> {
        let image = self.store.image().unwrap();
        let mut database = Connection::open_in_memory().unwrap();
        database
            .deserialize_read_exact(rusqlite::MAIN_DB, image.as_slice(), image.len(), false)
            .unwrap();
        let length: i64 = database
            .query_row(
                "SELECT length(CAST(statement_text AS BLOB)) FROM content_block WHERE id=?1",
                [block_id],
                |row| row.get(0),
            )
            .unwrap();
        for table in ["span_prefix", "span_member"] {
            let changed = database.execute(
                &format!("UPDATE {table} SET block_id=?1,start=?2,end=?3 WHERE local_id=?4 AND revision=?5"),
                params![block_id, blob(0), blob(length as u64), blob(prompt.local_id.0), blob(1)],
            ).unwrap();
            assert_eq!(changed, 1);
        }
        database.serialize(rusqlite::MAIN_DB).unwrap().to_vec()
    }
}

fn digest(bytes: &[u8]) -> Digest {
    Digest::new(format!("{:x}", Sha256::digest(bytes))).unwrap()
}

#[test]
fn adoption_binds_every_plan_task_prompt_to_its_designated_held_source() {
    for case in ["authorize", "operator_lock", "changed_task", "renewed_task"] {
        let mut fixture = Fixture::new();
        fixture.begin(case == "authorize");
        let prompt = match case {
            "changed_task" => fixture.replace(true),
            "renewed_task" => {
                fixture.complete();
                fixture.replace(false)
            }
            _ => span(2),
        };
        fixture.adopt(&fixture.store.image().unwrap()).unwrap();
        // Both duplicated prefix rows agree and the caller supplies the image's
        // actual hash. Only the immutable opening provenance can refuse this.
        let corrupted = fixture.retarget_prompt(&prompt, 1);
        let error = fixture.adopt(&corrupted).err().expect(case);
        assert_eq!(error.code, StateRefusalCode::OpenFailed, "{case}");
        assert!(
            error
                .detail
                .contains("task prompt differs from its opening source"),
            "{case}: {error}"
        );

        if case == "renewed_task" {
            let replacement_step = source_id(&fixture.store.connection, &block(5, 0))
                .unwrap()
                .unwrap();
            let corrupted = fixture.retarget_prompt(&prompt, replacement_step);
            assert!(
                fixture.adopt(&corrupted).is_err(),
                "renewal must retain the old prompt, not the new plan's first step"
            );
        }
    }
}

#[test]
fn adopted_model_lock_uses_authorized_task_without_allocating_a_prompt() {
    let mut fixture = Fixture::new();
    fixture.begin(true);
    fixture.file(statement(3, "message.assistant"));
    fixture.file(effect(
        4,
        PlanEffect::Lock {
            plan: plan(1),
            source: PlanLockSource::Model {
                authorization: block(1, 0),
                candidate: vec![reference(3, 0)],
            },
            task: TaskScopeRef {
                task: span(1),
                prompt: span(2),
            },
            steps: vec![PlanStepInit {
                step_id: step(1),
                instruction: reference(3, 0),
            }],
            review: setup(),
        },
    ));
    let adopted = fixture.adopt(&fixture.store.image().unwrap()).unwrap();
    let count: i64 = adopted
        .connection
        .query_row("SELECT COUNT(*) FROM span", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 3, "the model lock reuses the authorized scope");
    let lock = source_id(&fixture.store.connection, &block(4, 0))
        .unwrap()
        .unwrap();
    fixture
        .store
        .connection
        .execute(
            "UPDATE span SET opened_content_id=?1,
             ended_content_id=CASE WHEN local_id=?3 THEN ?1 ELSE ended_content_id END
             WHERE local_id IN (?2,?3)",
            params![lock, blob(1), blob(2)],
        )
        .unwrap();
    assert!(
        fixture.adopt(&fixture.store.image().unwrap()).is_err(),
        "a model lock may not become the task's opening source"
    );
}
