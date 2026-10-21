//! Timing begins at actual SQL validation, after image identity preflight.

use super::{ObservedAdoption, OpenFault, ServingSqlite, invalid};
use crate::{ReadFault, serving_clock::monotonic_ns};
use weaver_types::state_flow::*;

#[derive(Default)]
pub(super) struct QueryTiming {
    began: Option<DecimalU64>,
    ended: Option<DecimalU64>,
    failure: Option<std::io::Error>,
    #[cfg(test)]
    fail_end: bool,
}

pub(super) struct Guard<'a>(&'a mut QueryTiming);

impl QueryTiming {
    pub(super) fn start(&mut self) -> Result<Guard<'_>, OpenFault> {
        match monotonic_ns() {
            Ok(now) => self.began = Some(now),
            Err(error) => {
                self.failure = Some(error);
                return Err(invalid("adoption clock failed"));
            }
        }
        Ok(Guard(self))
    }

    fn finish(
        self,
        result: Result<ServingSqlite, OpenFault>,
    ) -> Result<ObservedAdoption, ReadFault> {
        let failure = |detail: String| ReadFault::ComponentFailure { detail };
        if let Some(error) = self.failure {
            return Err(failure(format!("adoption observation failed: {error}")));
        }
        let query = match (self.began, self.ended) {
            (None, None) => None,
            (Some(began_ns), Some(ended_ns)) if began_ns <= ended_ns => Some(OperationPhase {
                name: PhaseName::Query,
                began_ns,
                ended_ns,
            }),
            _ => return Err(failure("incomplete adoption observation".into())),
        };
        Ok(ObservedAdoption { result, query })
    }
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        #[cfg(test)]
        if self.0.fail_end {
            self.0.failure = Some(std::io::Error::other("injected end clock failure"));
            return;
        }
        match monotonic_ns() {
            Ok(now) => self.0.ended = Some(now),
            Err(error) => self.0.failure = Some(error),
        }
    }
}

impl ServingSqlite {
    pub fn adopt_observed(
        image: &[u8],
        common: &Common,
        schema_id: &str,
        scheme_id: &str,
        start: &StartIdentity,
        read_limits: ReadLimits,
    ) -> Result<ObservedAdoption, ReadFault> {
        let mut timing = QueryTiming::default();
        let result = Self::adopt_inner(
            image,
            common,
            schema_id,
            scheme_id,
            start,
            read_limits,
            Some(&mut timing),
        );
        timing.finish(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::serving_sqlite::image::{seed_image, sha256, tests::fixture};

    #[test]
    fn adoption_query_starts_after_preflight_and_covers_sql_refusals() {
        let seed = fixture();
        let image = seed_image(&seed).unwrap();
        let common = Common {
            session_id: "session".into(),
            run_id: "run".into(),
            request_id: 1.into(),
        };
        let mut start = StartIdentity {
            seed_id: seed.seed_id,
            artifact_id: "seed-fixture".into(),
            dump_sha256: sha256(image.as_bytes()),
            config_sha256: sha256(b"configuration"),
            parent_artifact_id: None,
            position: None,
        };
        let limits = ReadLimits {
            max_result_bytes: 1_000_000.into(),
            measurement_ref: "fixture only".into(),
        };
        let observed = |schema: &str, scheme: &str, start: &StartIdentity| {
            ServingSqlite::adopt_observed(
                image.as_bytes(),
                &common,
                schema,
                scheme,
                start,
                limits.clone(),
            )
            .unwrap()
        };
        let rejected = observed("wrong-schema", &seed.scheme_id, &start);
        assert!(rejected.result.is_err());
        assert!(rejected.query.is_none());
        start.dump_sha256 = sha256(b"wrong-image");
        let rejected = observed(STATE_CONTENT_SCHEMA, &seed.scheme_id, &start);
        assert!(rejected.result.is_err());
        assert!(rejected.query.is_none());
        start.dump_sha256 = sha256(image.as_bytes());
        let rejected = observed(STATE_CONTENT_SCHEMA, "wrong-scheme", &start);
        assert!(rejected.result.is_err());
        assert_eq!(rejected.query.unwrap().name, PhaseName::Query);
        let accepted = observed(STATE_CONTENT_SCHEMA, &seed.scheme_id, &start);
        assert!(accepted.result.is_ok());
        let query = accepted.query.unwrap();
        assert!(query.began_ns <= query.ended_ns);
        let mut timing = QueryTiming {
            fail_end: true,
            ..QueryTiming::default()
        };
        let result = ServingSqlite::adopt_inner(
            image.as_bytes(),
            &common,
            STATE_CONTENT_SCHEMA,
            &seed.scheme_id,
            &start,
            limits,
            Some(&mut timing),
        );
        assert!(result.is_ok());
        assert!(matches!(
            timing.finish(result),
            Err(ReadFault::ComponentFailure { .. })
        ));
    }
}
