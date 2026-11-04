# Primitives: organ and field audit

**Version:** 0.1, 2026-11-04
**Parent:** WeaverTools Working Process

## 1. Authority and reading boundary

This is the document audit requested by the operator in [#473][epic], item 1.
The decoder is the only required organ under that ruling. Other organs and
fields that serve one loop are elected in the binding. Absence means no use.
Whether the trace must accompany the decoder remains an operator decision.
Section 5 records both readings without choosing one.

This document records evidence and amendment scope. It changes no contract,
declaration, runtime behavior or workflow. Code follows merged organ amendments
and the workflow that uses them. R0 composition and frozen-kit searches stop.
PR #450 remains draft, unmerged, and held against new fixes without a ruling.

The source baseline is main at
`96bbc9c4b37e4bb1e7c4df788dc434edfb9be66a`.
The unmerged component stack at
`15dc3e5119f2c293a1300a9057f721f908c6fef1` is evidence of work in flight,
not authority or the current production implementation.

Tree M means the main baseline above. Tree U means the unmerged #450 head above.
Merged requirements in every column come from M. Code refusal rows name M or U.
An M coupling needs a new amendment against main. A U coupling also affects the
held stack. Its inclusion here grants no fix to #450. Clause changes must land
before either implementation changes.

Three states must be read separately. The existing executable uses the legacy
serving path. Merged state-management amendments describe its replacement.
Replacement components exist, but the coupled cutover has not occurred.
[FOUNDATION-TRANSITION][foundation] retains the old contracts until that cutover.
An implemented parser therefore does not prove that the executable selects it.

The audit covers seven organs, declaration leaves, admin provisioning keys,
governing artifacts, and loop-specific load and protocol fields. Related leaves
share a row only when their owner, presence rule and absence behavior are the same.
Ordinary protocol identifiers remain required inside an elected exchange.
Electing no exchange does not mean accepting malformed instances of it.

In the tables, "required" describes the cited baseline or merged clause.
"Elective" describes the operator's new rule, not an implemented omission path.
An absence that the current program cannot represent is stated as such.
The audit does not assign a replacement default, deadline or artifact.
An H label maps a row to section 2. A row without an H label resolves none of
the held subjects directly. Its election still belongs in the organ amendment.

## 2. The held questions as one dependency map

These labels identify the seven subjects in the [completion ledger][ledger].
They are not seven requests for separate R0 rulings. The [lexical correction][lex]
already puts H5 under H4's artifact dependency.

| ID | Held subject and source | Consequence of organ and field elections |
| --- | --- | --- |
| H1 | Receipt and placement `decision_wait`, [6622607475][wait] | A classifier-free binding must not acquire its transport bounds from a missing classifier. The active receipt or placement operation still needs a defined bound owner. Elective classification exposes this coupling rather than supplying a number. |
| H2 | Thesaurus declaration and bytes in TRACE-LOAD, [6622773400][trace-gap] | No thesaurus election means no lookup artifact to record. If lookup is elected, its load evidence still needs a defined representation wherever trace is required or elected. Absence does not authorize dropping evidence for an artifact that ran. |
| H3 | Root seed terminal result, [6623251698][seed-gap] | A binding without persistent state need not execute the state seed protocol. A binding that elects it still needs defined lost-result and cleanup outcomes. Optional state removes the universal prerequisite, not the selected protocol's failure duty. |
| H4 | Seedable SCHEME artifact, [6619214087][scheme-gap] | The decoder alone does not need an STE grammar or verb dictionary. Make the requirement conditional on the language and store features that consume it. Existing seed, binding and artifact verification cannot simply receive an empty scheme. |
| H5 | Lookup lexical inputs, [6627287175][lex-source] and [6627388356][lex] | No lookup or offer election means no lookup lexical reader. An elected lookup still needs the approved categories and reader owed by O02-1. It is part of H4, not a new generic artifact field. |
| H6 | Frozen 100-item kit locator, [6626634527][kit] | This is an external evaluation input, not an organ requirement. Stop the search. It is not a prerequisite for the organ audit or a fixture adapter load. |
| H7 | Complete R0 context, placement and GPU fit, [completion ledger][ledger] | The full loop fit is deferred with R0. Decoder and adapter admission still need their own resource measurements. A small fixture pass must not be reported as complete-kit fit. |

The common dependency is the mandatory serving arrangement, not seven missing
constants. The replacement binds state, scheme, prompt duties, classification
infrastructure and placement into one load. H1 through H5 expose different edges
of that arrangement. H6 and the complete-kit part of H7 belong to the deferred
evaluation. No item is marked resolved by this audit alone.

## 3. Organ inventory

| Organ | Required at the baseline, with authority | What refuses absence or prevents omission | Under #473, and held subjects affected |
| --- | --- | --- | --- |
| Gate | M: Required for legacy serving, excluded for diagnostic. [Apex 3 and 6][apex], [admin 4][admin], [types 2][types]. Replacement Serving still contains `gate_instruction` under TYPES-ENTER. | M: `inventory.rs::judge_config` rejects Serving without the instruction and Diagnostic with it as `ConfigInvalid(gate-instruction)`. Worker arguments and admin root still require a gate binary even when diagnostic never forks it. | Elective. No gate means no gate socket, admitted world input, external tool dispatch or gate request wait. Diagnostic proves a gate-free load arrangement exists. It does not prove that provisioning keys are already optional. H1, H7. |
| State | M: Legacy store absence defaults to sqlite. Explicit `engine = none` is supported when no state election accompanies it. Replacement serving requires sqlite and an indexed starting artifact. [Admin 4][admin], ADMIN-VALIDATION, TYPES-CONFIG and TYPES-PERSIST. | M: Legacy inventory rejects an election beside `none` and requires member provisioning when state stands. `parse_serving_draft` rejects `none` and missing state-management. Published parsing requires the start digest. Replacement adoption rejects missing or invalid artifacts. | Elective. No state means no member, semantic filing, state reads, snapshot, seed image or save-point requirement. The selected decoder's initial text source remains an amendment obligation. H1, H3, H4, H5, H7. |
| Trace | M: The serving trace is required by [apex 1 and 4][apex] and [trace 4 and 5][trace]. Diagnostic chooses a diagnostic recorder instead. Both require a sink. | M: `AgentConfig.trace_sink` has no optional form. `lifecycle.rs::enter_with_setup` rejects a missing sink as `DescriptorsUnusable`. The replacement recording context likewise has no absent-recorder type. | Open. Reading A elects trace like other organs. Reading B keeps it beside the decoder for the instrument's evidence duty. Both need the wording and custody consequences in section 5. H2 and recorded release obligations across H1 and H3. |
| SPU | M: Decoder instruction and model binding are required. Additional classifier and observation roles are distinct elections. [Types 2][types], [SPU 3, 5, 7, 8 and 11][spu]. | M: Common parsing rejects missing decoder/model members and empty device assignment. SPU admission rejects unsupported artifacts, families, capabilities or device room. The worker cannot start without its SPU binary argument. | Decoder remains required. Classifier, adapters, readout and other roles are elective. No adapter means base-only decode. No classifier means no classifier process, request or proof. A LoRA load path does not yet exist. H1, H4, H5, H7. |
| Admin | M: Required by the supported root-managed lifecycle. It judges custody, stands processes and exits after each verb. [Apex 6][apex], [admin 2, 6 and 9][admin]. | M: There is no binding key that omits admin. Its executable, root configuration, locks, credentials, descriptors and containment form the supported launch path. Missing configuration or failed custody refuses before launch. | Elective as an organ under the new rule. A decoder-only caller still needs a defined admission and ownership path. Removing admin must not silently grant root actions or weaken custody for bindings that use it. The alternate arrangement is not specified by this audit. H3, H4. |
| Harness | M: Required by the shipped worker composition. It owns lifecycle routing, recorder authoring and organ channels. [Apex 5.5][apex], [harness 1 through 4][harness]. | M: The worker constructs `Harness::listen` and runs `dev_loop::drive`. Admin requires `worker-binary`. No declaration omits the harness or substitutes an external decoder driver. Missing inherited descriptors and failed hygiene reject construction or entry. | Elective as an organ. Its routing mechanisms must remain usable without one fixed loop's policy. A direct decoder arrangement needs its own caller and contract obligations, not a hidden second loop in the harness. H1 through H5, H7. |
| Diagnostic | M: Its crate is linked into the harness. The diagnostic binding selects its recorder and replay interface, with no Gate. It is not the serving trace recorder. [Diagnostic 1 and 5][diagnostic], DIAGNOSTIC-CONVERT and DIAGNOSTIC-STATE. | M: No separate diagnostic process is required. Binding kind selects the path. Diagnostic setup still refuses a missing sink or missing decoder capabilities required by that replay. The diagnostic amendment excludes new serving event kinds. | Elective. No diagnostic election means no replay drive, column permission or diagnostic record obligation. Do not confuse omitting diagnostic with disabling an elected serving trace. No held R0 question requires diagnostic replay. |

The missing organ and the failed elected organ are different cases. The new rule
defines the former as unused. It does not permit an elected organ to crash and
be treated as if it had never been selected. Failure and shutdown contracts for
each elected arrangement still need complete state tables.

## 4. Field inventory

### 4.1 Common declaration and decoder

Paths in this table use the TOML spelling in `AgentConfig`. [Types 2][types]
defines the required-field rule. `config.rs` and admin inventory enforce it.

| Fields | Baseline requirement and refusal | Election meaning and audit consequence |
| --- | --- | --- |
| `session` | M: Required by types 2. Common parsing refuses a missing member. | Identity for the selected arrangement, not STE policy. Its role in a direct decoder binding must be stated. It does not imply state persistence. |
| `binding-kind` | M: Optional, absence resolves to Serving at inventory. Closed at Serving and Diagnostic. | A default serving composition conflicts with treating omitted organs as unused. Amend the binding model before introducing another kind or changing this default. |
| `spu-instruction.decoder.model-binding.artifact`, `.devices` | M: Required. Artifact syntax and nonempty device assignment are checked by parsing. The SPU owns file, family and device admission. | Remain decoder requirements. No scheme, classifier or state store follows from these fields. H7. |
| `spu-instruction.decoder.residual-readout-election` | M: Required boolean even when false. Missing member fails common parsing. SPU admission checks the elected readout capability. | Observation election, not a decoder prerequisite. Under the new absence rule, omission should mean no readout. The exact representation needs an amendment. |
| `spu-instruction.decoder.field-election.depth` | M: The table is optional. If present, depth is required. SPU 7.5 judges it against sampling and capability. | No table means no probability field. Retain checks for an elected field. |
| `spu-instruction.decoder.surprisal-election` | M: Optional and defaults false in types 2. | Already has the required unused meaning. It does not elect the diagnostic organ or another readout. |
| `spu-instruction.decoder.tunable-values` | M: Required map, possibly empty. Parsing validates supplied values. SPU 8 owns compiled parameter dispositions. | Decoder parameter mechanism, not an STE-specific field. Distinguish an omitted optional feature from a required resource or sampler value in an elected decoder. Do not invent new defaults. |
| `spu-instruction.decoder.refeed-permission`, `.column-permission` | M: Admin-owned wire facts. A declaration granting either is refused by inventory. Admin derives them from binding kind. | Diagnostic capabilities are elective and must not be granted by omission. Their producer changes only with the diagnostic binding amendment. |
| `spu-instruction.classify.model-binding.artifact`, `.devices` | M: Legacy classifier is optional by table presence. All members of a present binding are required. A missing executable for an elected classifier fails launch preparation. | No table means no legacy label scorer. Keep it distinct from the STE100 protocol. Moving both behind SPU roles is item 4, held. |
| Adapter path, digest, base identity and selection | M: No decoder adapter fields or LoRA load path exist at this baseline. Unknown declaration keys refuse. | Item 2 needs a separate SPU amendment. No placeholder binding key is introduced by the audit. Multi-adapter selection belongs to held item 3. H7. |

### 4.2 Gate, tools and lifecycle

| Fields | Baseline requirement and refusal | Election meaning and audit consequence |
| --- | --- | --- |
| `gate-instruction.access-rule.allowed-uids`, `.allowed-gids`, `.denied-uids` | M: Required for Serving, excluded for Diagnostic. Admin resolves admitted users and groups. ADMIN-STE100 adds operator-only access without a classifier under Q6. | Only an elected gate needs an access rule. Its absence grants no alternative input route. Preserve Q6 for the current no-classifier STE workflow until its amendment. |
| `tool-set` | M: Absence is an empty list. Unknown or duplicate tools refuse. The registered external tool is `sh`. | Already elective. No tools means no dispatch, shell grant or tool policy. Q7 remains the current STE loop's prohibition. |
| `state-management.tools.sh.clock-ms`, `.measurement-ref` | M: Required exactly when `sh` is elected. `ToolPolicies::validate` rejects missing or surplus policy. The clock must be within the shared shell maximum. | Tool policy belongs to the elected tool capability, not mandatory state or classification. Changing the current no-classifier prohibition needs a workflow and contract act. |
| `shell.binary`, `.commands` in Serving Enter | M: Optional grant constructed by admin, with judged executable and command-list paths. It is not caller-chosen text. | Absent tool capability means no shell grant. An absent gate cannot acquire dispatch through a retained shell field. |
| `lifecycle.drain-bound`, `.wind-down-bound` | M: Each is optional. Absence means unbounded graceful wait, with force-unload as recourse. Present values are checked by common parsing. | These govern an elected lifecycle. Their current absence already has a ruled meaning. They must not become substitute receipt or placement bounds. H1. |
| `state-management.flow.gate-wait.elapsed-ns`, `.measurement-ref` | M: Required by TYPES-CONFIG. The grammar requires a positive wait and a reference. HARNESS-CLOCK starts it at check intake. | Only the arrangement that admits through this gate/check path needs it. Gate absence does not justify an unbounded elected input check. H1. |
| `state-management.flow.cut-wait.elapsed-ns`, `.measurement-ref` | M: Required by TYPES-CONFIG. HARNESS-CLOCK uses it at the between-turn filing barrier. | Conditional on that barrier. A decoder-only binding has no semantic filing cut to wait for. A selected barrier retains a defined failure bound. H1. |

### 4.3 State, persistence and placement

The replacement paths below are required even when `ste100` is absent under
the merged TYPES-CONFIG. This is the principal coupling the audit identifies.

| Fields | Baseline requirement and refusal | Election meaning and held subject |
| --- | --- | --- |
| `state-store.engine` | M: Legacy absence means sqlite. Explicit `none` is accepted without `state-election`. Replacement serving refuses `none`. | State must become elective without a hidden sqlite default. Absence means no member or state duties. H3, H4. |
| `state-election.all-kinds`, `.keys[].kind`, `.keys[].paths` | M: Legacy and Diagnostic election. An absent election resolves to the envelope of every kind. Both members are required if the election is written. An election beside engine none refuses. | Only an elected legacy/diagnostic store needs this projection. Absence of the organ must not instantiate the default tee. |
| `restore.save-point` | M: Optional legacy restore. Present names are checked at inventory. The replacement serving parser rejects the old table. | Restore is conditional on persistence. Do not turn absence into an empty restored image in the new serving path. H3. |
| `state-management.flow.version` | M: Required integer 1. `FlowConfig::validate` and the grammar reject other values. | Version of an elected flow protocol, not the decoder. |
| `state-management.flow.partition[].id`, `.kind`, `.channels`, `.origins`, `.blocks`, `.context` | M: Partition is required, but may be empty. The five first row members are required. Context is optional. Grammar checks names, uniqueness and projection syntax. Trace-aware validation checks tuple vocabulary and overlap. | Conditional on semantic filing. No classifier already means no Text filing under HARNESS-PUMP's preceding ledger clause. Empty partition is not proof of an absent state organ. H4. |
| `state-management.flow.backlog.max-pending-blocks`, `.max-pending-bytes`, `.lag-alarm-turns`, `.measurement-ref` | M: All required and positive where numeric. `FlowBacklog::validate` rejects missing or invalid limits. | Conditional on the selected filing queue. Preserve bounded buffering when the queue exists. Do not carry a fake backlog into a decoder-only declaration. |
| `state-management.flow.observation-interval.elapsed-ns`, `.measurement-ref` | M: Required positive cadence under TYPES-CONFIG and Spec 09 FLOW-COUNTS. It is not a deadline. | Conditional on the observed flow. Never borrow this value for H1. |
| `state-management.start.dump-sha256` | M: Optional in the authoring draft, required in the published declaration. `parse_serving_declaration` refuses absence. ADMIN-VALIDATION requires an indexed verified artifact. | Conditional on persistent state. A decoder-only arrangement does not need a seed dump merely to load its model. Its initial text route remains to be specified. H3, H4. |
| `state-management.persistence.save-wait.elapsed-ns`, `.measurement-ref` | M: Required. Grammar and `PersistenceConfig::validate` refuse absence or an invalid bound. | Conditional on the selected persistence protocol. A selected save still needs bounded completion and rollback. H3. |
| `state-management.persistence.snapshot-wait.elapsed-ns`, `.measurement-ref` | M: Required by TYPES-CONFIG and PERSIST-EXPIRY. Foundation Spec section 5 names 120 seconds as the initial explicit setting. The operator's declaration carries it. No provisioning step writes it. | Conditional on snapshots. The initial setting is not a universal primitive deadline or benchmark. H3. |
| `state-management.reads.max-result-bytes`, `.measurement-ref` | M: Required complete-result bound under STATE-PAGES. Grammar checks size and reference. | Conditional on state reads. A selected read retains complete-result refusal rather than partial success. |
| `state-management.placement.c-head`, `.c-tail`, `.measurement-ref` | M: Required positive capacities under Spec 06 CAPACITY. `PlacementConfig::validate` rejects zero values and overflow. | Conditional on that head/tail placement mechanism. The decoder still has a finite model context without that algorithm. H1, H7. |
| `StartIdentity.seed_id`, `.artifact_id`, `.dump_sha256`, `.config_sha256`, `.parent_artifact_id`, `.position` | M: Required shape under TYPES-PERSIST. Nullable lineage fields remain present. State and admin verify the identity against the artifact. | Conditional on persistent lineage. No state does not mean fabricating an initial artifact identity. H3. |

### 4.4 Language, scheme, lookup and certificates

| Fields | Baseline requirement and refusal | Election meaning and held subject |
| --- | --- | --- |
| `state-management.ste100.model-binding.artifact`, `.devices` | M: Entire STE100 table is elective under DECLARATION and TYPES-CONFIG. A present table requires an absolute manifest path and valid devices. `Ste100Instruction::validate` enforces the shape. | No classifier means no instruction or manifest load. Item 4 may move this role later. It is not the legacy classify binding. H4. |
| `state-management.ste100.model-artifact-sha256` | M: Required digest of the classifier manifest for a present instruction. Bootstrap verifies the artifact identity. | Only an elected classifier needs this manifest identity. It is not the checker executable or scheme hash. H4. |
| `state-management.ste100.instances`, `.max-wire-bytes`, `.measurement-ref` | M: Required positive resource limits and nonempty reference for a present instruction. Structural validation refuses invalid values. | Conditional on the STE100 duty. Removing it must remove its queues and logical-message obligations. |
| `state-management.ste100.repair-cap.attempts`, `.measurement-ref` | M: Required positive classifier-repair bound under DECLARATION. | Conditional on classifier repair. It is not the external STE driver's three-request limit or offer cap. |
| `state-management.ste100.decision-wait.elapsed-ns`, `.measurement-ref` | M: Required inside a present instruction. HARNESS-CLOCK also uses it for non-classifier send receipts and placement. Section 8 lists ten wait consumers and one trace mirror. | Split the ownership question by active operation, not by an R0 default. A classifier election cannot supply the only bound for unrelated elected work. H1. |
| `state-management.flow.scheme.scheme-id`, `.artifact-sha256`, `.verbs` | M: Required even without STE100 under TYPES-CONFIG and SCHEME. Grammar and scheme verification reject missing or mismatched identities and verb keys. | Conditional on consumers of the language instance, including an elected lexical store or lookup. No classifier alone does not prove there are no other consumers. H4, H5. |
| `state-management.flow.thesaurus.artifact-sha256`, `.scheme-id` | M: Elective under THESAURUS. If present, its scheme identity must match. Main's FlowConfig and grammar do not yet carry it. #450 adds those unmerged carriers. | No election means no lookup change or thesaurus load. An elected artifact retains exact provenance and binding checks. H2, H5. |
| Scheme `format`, `reference.edition`, `.document_sha256`, `.rule_ids`, `.dictionary_sha256` | M: Required by SCHEME. Strict decoding and scheme validation refuse missing, unknown or mismatched fields. | Conditional governing artifact for the selected language features. A digest names evidence and does not contain the lexical tables. H4, H5. |
| Scheme `grammar.format`, `.bytes_base64`, `.sha256`, and the same three `conditions` members | M: Required by SCHEME and SEED-CERTIFICATE. Canonical decoding and digest checks precede use. Compiler and normative qualification are O02-1. | Conditional on their consuming language functions. Elective organs do not license fabricated grammar bytes or a test-only scheme in a qualified binding. H4, H5. |
| Scheme `verbs[].key`, `.forms`, `.sense`, `.authority.rule_id`, `.authority.disposition` | M: Required closed dictionary under SCHEME. Keys and forms have uniqueness constraints. Declaration verbs must match in order. | Belong to elected language semantics and lexical filing, not generic decoder loading. H4, H5. |
| Scheme `technical_names[].text`, `.authority.rule_id`, `.authority.disposition`, and `recall_keys` | M: Required SCHEME fields. Recall keys are a nonempty subset of verb keys. | Technical-name checking and noun-history recall are selected features. Their grammar must not remain a compulsory decoder prerequisite. H4, H5. |
| Scheme `qualification.report_sha256`, `.corpus_sha256`, `.checker_abi` | M: Required qualification record under SCHEME. A fixture edition is refused by production checker admission. | Required for the elected qualified language duty, not proof that every decoder must classify. H4. |
| Thesaurus `format`, `scheme_id`, `source.repository`, `.commit`, `.path`, `.sha256`, `entries[].word`, `.tag`, `.senses[].sense`, `.use` | M: Required by THESAURUS when the artifact is selected. Strict codec and tag/cardinality checks refuse malformed entries. These carriers are in #450, not main. | Conditional on lookup. No election means no artifact, detector or offer operation. The approved lexical reader remains a dependency where selected. H2, H5. |
| Seed certificate `prompt_sha256`, `scheme_id`, `checker_artifact_sha256`, `review_reference`, `bindings`, `duties` | M: SEED-CERTIFICATE requires exact prompt and scheme checks even without a classifier. The merged amendment makes checker digest null and bindings empty for that election. Main's certificate still requires a digest. #450 implements the nullable case. | State seed, prompt review and semantic certification need separate presence rules. A model's initial text must not require irrelevant language duties. Exact reviewed bytes still matter where that certificate is elected. H3, H4. |
| Seed duties `ste100_emission`, `recall`, `forward_flags`, `locked_plan`, `review_closure`, `restatement`, `ste100_input_history`, `initial_plan`, `wall_summary` | M: PROMPT-3 and `deserialize_duties` require the closed nine-duty set. These describe the state-management loop, not generic decoding. | Elect each duty with the operation that uses it. Their absence must not reject a decoder-only prompt. Retain the current certificate contract until amended. H4. |
| Seed request `schema_id`, `seed_id`, `scheme_id`, `ste100_scheme`, `checker_artifact_sha256`, `prompt_text`, `raw_block`, `bindings`, `certificate`, `ste100_thesaurus` | M: TYPES-SEED and STATE-SEED verify schema, exact Text bytes and certificate before insertion. Main's request lacks `ste100_thesaurus` and the nullable checker shape. | Whole exchange is conditional on state seeding. A selected exchange keeps its strict shape, cleanup and publication ordering. Do not send a partial request to represent absent state. H3, H4, H5. |

### 4.5 Trace and diagnostic fields

| Fields | Baseline requirement and refusal | Election meaning and held subject |
| --- | --- | --- |
| `trace-sink.kind`, `.path`, and `.create` for file or pipe | M: Required sink choice under types 2 and admin 5. There is no none variant. Missing fields fail parsing. Missing inherited sink fails entry. | Conditional under trace-floor reading A. Required for the selected evidence recorder under reading B. Do not substitute a discarded sink while reporting trace custody. |
| Trace `state_flow` with registry, version, partition, bounds, backlog, scheme, persistence, reads, STE100, tools and placement, plus `state_start` | M: TRACE-LOAD mirrors the serving declaration. Main's `FlowDeclarationRecord` has a concrete STE100 record. Its checked load constructors and template inventory expect checker identity. #450 relaxes classifier facts, but the thesaurus evidence shape remains absent. | A trace records selected capabilities. It must not demand classifier, state or lookup evidence for an unused capability. When selected, those facts must be complete. H2, H4. |
| `ste100_binary_sha256`, template inventory `checker_artifact_sha256`, inventory digest and certification results | M: Merged TRACE-LOAD records no checker digest or template-check result without a binding. Main still has non-null checker fields in the serving record shapes. | Classifier proof is conditional. Structural template identity may still be required for an elected template operation. Absence is not a fabricated successful proof. H4. |
| Trace session, run, turn, sequence, timestamps, origin, subsystem and causal parent | M: The selected record's schema and licensing determine required or nullable fields. Trace 3 through 5 and event admission enforce them. | These are evidence mechanics inside an elected trace, not separate organ elections. Trace absence and malformed trace records are different cases. |
| `loop.instruction` template and metadata, `emission.change` match and source links | M: TRACE-INSTRUCTION and TRACE-EMISSION-CHANGE license the selected operations. The latter and the offer variant have unmerged representations in #450. Harness provenance remains a separate duty. | Conditional on those loop operations. No lookup means no Offer instruction or change evidence. The deferred source-order consistency proposal on #437 is not taken into #450. H2, H5. |
| Diagnostic event payloads, readout vectors, field/surprisal readings, replay and column references | M: Diagnostic 3 and 5 define the closed diagnostic record. DIAGNOSTIC-CONVERT refuses serving-only vocabulary. SPU capability checks gate elected observations. | Conditional on diagnostic or observation elections. No diagnostic means no diagnostic recorder. Selecting another observation does not implicitly enable the whole replay arrangement. |

### 4.6 Admin provisioning, entry and loop operations

| Fields or operation family | Baseline requirement and refusal | Election meaning and held subject |
| --- | --- | --- |
| Root `worker-binary`, `spu-binary`, `gate-binary`, `coordination-root`, `territory`, `operator`, `roles.toml` | M: Required by admin 9 for the supported lifecycle. Root reads and custody checks fail before launch. Roles are required for validate/load, with separate existing behavior for stopping a damaged run. | Keep provisioning conditional on the elected organ and custody arrangement. Gate-free diagnostic still has a mandatory gate-binary root key. No admin means a different specified launcher, not skipped checks inside this one. |
| Conditional provisioning: root `ste100-binary` and `shell-binary`, derived classifier/member/relay binaries, member account and roles-file trace reader | M: ADMIN-STE100 and admin 4, 6 and 9 govern presence. The legacy classifier binary is derived beside the worker. If absent, admin omits the flag and harness admission refuses an elected classifier. Missing elected state provisioning refuses at inventory. No STE100 binding removes its root binary requirement. | No elected capability means no requirement to provision or start that executable. State and trace reader/relay requirements must follow their own elections, not the decoder's. H2, H3, H4. |
| Root `headroom-bytes`, `library-path`, `load-bound-seconds` | M: Optional under admin 9. The supported load bound has an existing 900-second default. Path and value checks apply when written. | Runtime resource and lifecycle facts. Do not reuse the load bound as decision_wait. Keep the load owner's existing bound until its protocol changes. H1, H7. |
| Root `restore-artifact`, `ste100-scheme.json`, `ste100-thesaurus.json`, territory prompt and certificate | M: ADMIN-SELECTION chooses an indexed artifact. ADMIN-LOAD uses its governed files. ADMIN-SEED reads root sources and the exact reviewed prompt. Missing required selected inputs refuse. | Conditional on state publication, language and prompt certification. Their removal does not define the initial text source for a decoder-only binding. H3, H4, H5. |
| Enter `session`, `run`, `spu_instruction`, `binding`, `state_store`, `declaration`, `restore`, `reset`, `stack`, `boundary`, `cause`, `operator`, `library_path`, `drain_bound`, `wind_down_bound` | M: TYPES-ENTER and the existing lifecycle wire define strict required and nullable shapes. The common channel rejects malformed frames and wrong descriptors. | Some are shared lifecycle evidence, others belong to state, custody or selected organs. Amend the enclosing binding variants together. Omitting an organ does not grant malformed Enter frames. |
| Serving Enter `enter_window.began_ns`, `.deadline_ns`, `gate_instruction`, `shell`, `state_management`, `start`, `ste100_scheme`, `ste100_thesaurus` | M: TYPES-ENTER and TYPES-ENTER-WINDOW require the replacement serving shape. Diagnostic excludes these serving fields. The complete envelope still has its existing bound. Main lacks the thesaurus member. | Split by elected capability and preserve one load owner's deadline where that lifecycle is used. Never smuggle absent state through a fake start or scheme. H1 through H5. |
| State filing `bindings`, statement context/effect, plan and mark references, review setup and forward flags | M: TYPES-FILE, TYPES-EFFECT, Spec 06 and Spec 07 define operation-specific shapes. Store admission checks identity, attribution and declared vocabulary. No classifier already means no semantic Text filing. | Conditional state features. Their presence on a selected request remains checked. An absent plan workflow should not impose plan or review duties on all prompts. H4. |
| Selection criterion, material references, ordering, drop records, placement journal and mark positions | M: Spec 08 SORT-PORT and Spec 06 PLACE-PORT define typed mechanisms. Private constructors and refusal paths prevent arbitrary placement or incomplete accounting. | Elected loop mechanisms. These are not mandatory decoder admission facts. Preserve their ownership and invariants when selected. H1, H7. |
| R0 `driver_repair_pass_cap`, shared request limit, `offer_pass_cap`, `transport_stop_bound`, prompt/kit pins, review sample and loss metrics | M: Workflow 4 through 7 and WeaverTrain #15 govern the external evaluation and selected conversion loop. #450 carries unmerged implementation subsets. | Remain workflow/evaluation policy. They do not become default organ fields. Stop work on their runtime composition and kit dependencies under #473. H6, H7. |

### 4.7 Refusal changes in the unmerged tree

These rows supplement the M inventory. U means only the pinned #450 tree,
including the collapsed component PRs. It is not a merged or runnable R0 claim.
They answer the review seat's source-boundary request in [6635146061][audit-review].

| Tree and fields | Refusal site and difference from M | Clause, election and held subject |
| --- | --- | --- |
| U: optional checker, required scheme, prompt and certificate | `weaver-admin/src/seed/preparation.rs::prepare_against` reads the checker only when STE100 is elected. It still reads the scheme, prompt and certificate unconditionally. Certificate binding/election disagreement returns `PreparationFailure::Certificate`. M has no such seed preparation module. | ADMIN-SEED, ADMIN-STE100, SEED-CERTIFICATE. This dormant path removes checker provisioning without removing state and language prerequisites. H3, H4. |
| U: exact seed bytes, nullable checker and nine duties | `weaver-types/src/state_flow/persistence.rs`, SeedRequest::validate_structure and verify_seed_certificate reject wrong schema, raw Text, scheme, digest, bindings or duties. Missing nullable members fail strict decoding. M requires a non-null checker and has no thesaurus member. | TYPES-SEED and TYPES-PERSIST. Optional classification is implemented here. Optional seed certification or language duties are not. H3, H4, H5. |
| U: elective thesaurus declaration and bytes | `weaver-types/src/state_flow/thesaurus.rs`, ThesaurusDeclaration::validate and Ste100Thesaurus::validate reject wrong scheme, provenance, digest or entry shape. `weaver-types/src/config/serving/grammar.rs` now accepts the table. M cannot parse this election. | THESAURUS and TYPES-CONFIG. A selected artifact remains checked. Its absence does not fill H2's trace representation. H2, H5. |
| U: scheme and thesaurus publication/load files | `weaver-admin/src/save_points/serving/governing.rs::GoverningFiles::verify` rejects a malformed scheme or disagreement between thesaurus election and bytes. `weaver-admin/src/serving_enter.rs::PreparedEnter::prepare_against` accepts no checker and uses verified saved governing bytes. M's entry still rejects no checker. | ADMIN-LOAD, STATE-GOVERNING and C3/Q8. These are component changes, not the executable's coupled cutover. State and a scheme remain required. H2, H3, H4, H5. |
| U: selected thesaurus during harness startup | `weaver-harness/src/flow/context_enter_startup.rs::StartupComposition::from_enter` rejects a thesaurus declaration or bytes with the state-content/1 startup refusal. It then still requires a classifier, as M does. | FOUNDATION-TRANSITION and HARNESS-PUMP. Carriers alone do not provide a no-classifier recording context. This remaining integration stays held. H1, H2, H4. |
| U: trace load and template checker facts | `weaver-trace/src/serving/payloads/records.rs` permits absent checker facts. Its checked load path still compares the complete flow, whose thesaurus evidence is unrepresented. M has concrete checker fields. | TRACE-LOAD. This is H2's remaining representation question, not permission to omit evidence for a used artifact. H2, H4. |

All paths in this table start at `crates/`. Changes to these U sites would reshape
the held stack and require the operator's ruling. The audit adds no such changes.

### 4.8 Existing transport limits

These are existing limits read in both M and U. They are not new declarations or
values proposed for H1. The enclosing operation decides whether each is relevant.

| Wire | Required bound and refusal | Source and election consequence |
| --- | --- | --- |
| Gate world line | M and U: 32 KiB before the newline. An oversized line closes the connection. | Gate Spec 4, `crates/weaver-gate/src/relay.rs::LINE_BOUND`. Applies only when that gate path is used. |
| Admin and organ envelope | M and U: 64 KiB. Oversized receive truncation is a channel fault. Replacement Enter encoding also checks this bound. | Types Spec 4 and TYPES-ENTER, `crates/weaver-types/src/wire.rs::MAX_ENVELOPE_BYTES` and `state_flow/enter.rs`. Removing an organ does not increase the envelope limit. |
| State and seed request line | M and U: 8 MiB including LF. The strict codec refuses oversized frames. | TYPES-CODEC and TYPES-SEED, `crates/weaver-types/src/state_flow/codec.rs::STATE_REQUEST_LINE_BOUND`. Conditional on the elected state protocol. |
| State answer line | M and U: 1 MiB including LF. The strict codec refuses oversized answers. | Same codec, STATE_ANSWER_LINE_BOUND. It is not the request bound. |
| SPU decode logical message | M and U: up to 8 MiB, segmented through envelopes bounded at 64 KiB. Faulty or oversized series refuse. | Types Spec 4.4, `crates/weaver-types/src/wire.rs::DECODE_MESSAGE_BOUND`. This is the decoder transport's bound, not a model token capacity. H7. |

The approved folded prompt is 47,214 UTF-8 bytes. Its JSON string is 48,404 bytes
when encoded without ASCII substitution, including the string quotes. Its SHA-256
is `0fe2ee35a59f6b1e30734af4d7a72e32bd6dbc0e728abc98e32e2c609dad07ed`.
That string alone exceeds the gate line limit. It is below the seed request limit,
but the whole request also contains the raw Text copy, scheme and certificate.
These sizes do not prove full-frame admission, model fit or a seed run.
No alternate prompt route or larger limit is elected by this comparison.

## 5. The two trace-floor readings

The word floor has two meanings in this discussion. [Apex 5.1][apex] calls only
weaver-traits and weaver-types the dependency floor. It explicitly excludes
weaver-trace from that definition. The new question is which organs a valid run
must contain. A runtime requirement need not change Cargo dependency ownership.

Reading A applies the decoder-only rule to trace too. A binding may elect no
trace. It then has no recorder, sink, tee, trace-derived state or claim of a
replayable trace. This permits the smallest decoder instrument. It needs changes
to the apex deliverable, the missing-sink refusal, Recorded-based grants and
the supported lifecycle. A discarded stream is not an implementation of absence.
The amendment must define what authorizes progress where Recorded is not present.

Reading B keeps a trace beside every decoder run. This preserves the primary
artifact and reproducibility duties in apex 1 and 4. The review seat argues for
this reading in #473. It makes a deliberate exception to the otherwise elective
organ rule. The minimum record must identify the actual base, adapter election,
request, result and execution context without requiring state or a classifier.
Exact replay may also depend on retained artifacts and backend determinism.
A trace by itself does not prove bitwise replay.

The operator must choose between these readings and state whether a diagnostic
record satisfies the minimum evidence duty. Neither option implies that a state
member, semantic filing or the nine prompt duties must stand. Until the ruling
and amendments land, the existing sink and recorder requirements remain in force.

## 6. Amendment boundaries exposed by the audit

The following are a proposed grouping for review. They are not implementation
authorization or a new sequence of R0 answers.

| Group | Documents and seams that must change together | Decision or evidence needed before code |
| --- | --- | --- |
| A. Binding and organ absence | Apex lifecycle and scope, types configuration and Enter, admin inventory/start, harness lifecycle, organ contracts | State exactly which organs and fields are elected, how the caller drives a decoder-only arrangement, and what absence means at each endpoint. Keep failed elected organs distinct. |
| B. Evidence floor | Apex deliverable, trace and diagnostic contracts, recorder ownership, Recorded-based continuation grants | Operator trace-floor ruling. State evidence and custody duties for each permitted arrangement. |
| C. Capability-owned state and language | TYPES-CONFIG, SCHEME, SEED-CERTIFICATE, ADMIN-SEED/LOAD, STATE-GOVERNING/ADOPT, TRACE-LOAD, prompt duties | Separate optional state, language, lookup and certification consumers. Preserve exact bytes and identities when elected. Define any initial-text path without state. This re-examines H2 through H5 together. |
| D. Bounds owned by active operations | HARNESS-CLOCK, decision/placement/receipt contracts, shared types and qualification | A bound owner independent of an absent classifier. Keep finite-wait and completion semantics where the operation exists. No amount is selected here. H1. |
| E. Fixture adapter loading | SPU PRD/Spec, model binding and admission vocabulary, harness-SPU contract, evidence for the selected backend | Item 2's separate amendment. Operator fixture source and accepted drift. #435 owns conversion and target coverage. |

Items 3 and 4 remain held behind items 1 and 2. The audit does not implement
multi-adapter scheduling, per-role sockets or the classifier's new binding home.
Their anticipated ownership helps identify coupling but does not settle their API.

## 7. Open cells and verification

| Cell | Owner and measurement or decision that settles it |
| --- | --- |
| Trace in the runtime floor | Operator ruling on the two readings in section 5, including diagnostic evidence and Recorded-based continuation. |
| Initial text without persistent state | Binding amendment specifying the source, custody and lifetime of decoder input. No new prompt field or filesystem reader is elected here. |
| 4B fixture adapter source | Operator identifies the compatible artifact or authorizes its production/download. Record base and adapter digests before any load test. |
| Accepted Python/SPU drift | Operator chooses the comparison and tolerance. Measure both base-only and adapter-enabled paths with pinned prompts, settings and artifacts. A changed output proves application, not accepted parity. |
| Amendment scope | Planner review of this inventory and the operator's grouping decision. Changes reach every party to each altered seam before code. |

The current audit is a source reading, not a successful absent-organ run.
Evidence includes the strict declaration grammar, admin inventory and entry,
worker composition, recorder construction, and the matching merged clauses.
The diagnostic path supplies the existing gate-free case. It does not establish
that a decoder-only binding or trace-free binding can run.

After the amendments, the implementation must test each organ omitted alone,
the permitted minimal binding, and a failed elected organ. It must also test
required fields inside an elected operation, not just top-level table absence.
Tests must fail when the election or refusal property is removed.

The SPU fixture work must prove that the adapter is applied, rather than merely
read. The operator has not chosen that fixture or accepted a drift value.
The 30B family entry and R1 emitter adapter test remain the olympus lane.
The review seat's WeaverTrain #21 checks remain read-only and separate.

## 8. Source index

The clause IDs refer to the merged [types amendment][types-amend],
[admin amendment][admin-amend], [harness amendment][harness-amend],
[state amendment][state-amend], [trace amendment][trace-amend] and
[diagnostic amendment][diagnostic-amend]. Language and certificate clauses are
in [Spec 02][protocol]. Placement, plan, selection and flow accounting clauses
are in [Spec 06][placement], [Spec 07][plans], [Spec 08][selection] and
[Spec 09][qualification]. The deferred loop policy is in the [STE workflow][loop].

Code paths below are relative to the pinned repository root. They name the
entry or type inspected, not a claim that every replacement component is active.

| Evidence | Source path and site |
| --- | --- |
| Common declaration and absence defaults | M: `crates/weaver-types/src/config.rs`, AgentConfig, DecoderInstruction, TraceSink and StoreEngine |
| Replacement required tables and leaves | M: `crates/weaver-types/src/config/serving.rs`, parse_serving_draft and parse_serving_declaration. `config/serving/grammar.rs`, Object::value and parse_state |
| Bounds and state election structure | M: `crates/weaver-types/src/state_flow/declaration.rs`, FlowConfig, StateManagementConfig and validators |
| Language artifacts and classifier shape | M: `crates/weaver-types/src/state_flow/ste100.rs`, Ste100Instruction, Ste100Scheme and validators |
| Prompt duties and certificate | M: `crates/weaver-types/src/state_flow/persistence.rs`, PromptDuty, SeedCertificate, SeedRequest and deserialize_duties |
| Replacement lifecycle carriers | M: `crates/weaver-types/src/state_flow/enter.rs`, EnterBinding and LoadedStateFacts |
| Gate and store absence | M: `crates/weaver-admin/src/inventory.rs`, binding match, store match and member provisioning checks |
| Required provisioning and active dispatch | M: `crates/weaver-admin/src/main.rs`, ServiceConfig and dispatch. `crates/weaver-admin/src/surface.rs`, Request and parse_arguments |
| Retained classifier-only entry | M: `crates/weaver-admin/src/serving_enter.rs`, PreparedEnter::prepare_against |
| Static worker composition | M: `crates/weaver-harness/src/bin/worker/main.rs`, main and dev_loop::drive selection |
| Sink required for either recorder | M: `crates/weaver-harness/src/lifecycle.rs`, enter_with_setup |
| Classifier-coupled startup | M: `crates/weaver-harness/src/flow/context_enter_startup.rs`, StartupComposition::from_enter |
| Initial snapshot setting and its renderer | M: `docs/crates/state-management/state-management-foundation-Spec.md`, section 5, lines 719-720. `crates/weaver-admin/src/save_points/serving/declaration.rs`, lines 285-288, renders the supplied value |
| Trace load representation | M: `crates/weaver-trace/src/serving/payloads/records.rs`, FlowDeclarationRecord and TemplateInventory |

The complete M census of `decision_wait` consumers follows. Paths start at
`crates/weaver-harness/src/flow/`. Ten sites use the duration for a wait.
The eleventh copies the instruction into trace evidence and starts no wait.
These are inputs to Group D's ownership amendment, not new bound elections.

| File | Lines on M | Consumer |
| --- | --- | --- |
| `context_boot_pairs.rs` | 537 | Boot pair receipt wait, capped by Enter and Leave |
| `context_input.rs` | 407 | Required world-close send interval |
| `context_pair_execution.rs` | 519, 550, 619 | Terminal wait, next-send wait and rejection wait |
| `context_place_generation.rs` | 392 | Decision window before output processing |
| `context_plan_update.rs` | 299 | Root plan acceptance window |
| `context_repair.rs` | 1279 | Repair decision window |
| `context_worker_generation.rs` | 890 | Generation Decision window when none is armed |
| `context_worker_plan_refusal.rs` | 39 | Plan-refusal response window |
| `mirror.rs` | 116-137 | Trace instruction mirror, including duration and measurement reference |

This source search reproduces the census on M. The mirror has several matches
for one consumer. The excluded fixture and test files do not run in service.

```sh
rg -n 'decision_wait' crates/weaver-harness/src/flow \
  --glob '*.rs' --glob '!**/*test*' --glob '!recording_fixture.rs'
```

[epic]: https://github.com/toddwbucy/WeaverAgent/issues/473
[ledger]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6627363218
[lex]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6627388356
[wait]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6622607475
[trace-gap]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6622773400
[seed-gap]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6623251698
[scheme-gap]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6619214087
[lex-source]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6627287175
[kit]: https://github.com/toddwbucy/WeaverAgent/issues/437#issuecomment-6626634527
[audit-review]: https://github.com/toddwbucy/WeaverAgent/issues/473#issuecomment-6635146061
[foundation]: ../crates/state-management/state-management-foundation-Spec.md
[apex]: ../crates/weaver-agent-PRD.md
[types]: ../crates/weaver-types/weaver-types-Spec.md
[admin]: ../crates/weaver-admin/weaver-admin-Spec.md
[harness]: ../crates/weaver-harness/weaver-harness-Spec.md
[spu]: ../crates/weaver-spu/weaver-spu-Spec.md
[trace]: ../crates/weaver-trace/weaver-trace-Spec.md
[diagnostic]: ../crates/weaver-diagnostic/weaver-diagnostic-Spec.md
[types-amend]: ../crates/state-management/weaver-types-Spec-amendment.md
[admin-amend]: ../crates/state-management/weaver-admin-Spec-amendment.md
[harness-amend]: ../crates/state-management/weaver-harness-Spec-amendment.md
[state-amend]: ../crates/state-management/weaver-state-Spec-amendment.md
[trace-amend]: ../crates/state-management/weaver-trace-Spec-amendment.md
[diagnostic-amend]: ../crates/state-management/weaver-diagnostic-Spec-amendment.md
[protocol]: ../crates/state-management/state-management-spec-02-protocol-classifier-gate.md
[placement]: ../crates/state-management/state-management-spec-06-marks-renderer-engine.md
[plans]: ../crates/state-management/state-management-spec-07-plan-forward-flags.md
[selection]: ../crates/state-management/state-management-spec-08-sort-selection.md
[qualification]: ../crates/state-management/state-management-spec-09-qualification-costs.md
[loop]: ../crates/weaver-harness/Loops/ste-conversion-loop.md
