# State management worker generation composition

Date filed: 2026-10-23
Branch: `code/state-worker-generation-composition`
Issue: #146
Parent: #306 `288cbe96745f8538017fefa5af92a5171d769af7`
Status: completed implementation slice for draft review; goal incomplete.

## Composition and authority

This joins the actual entered worker's accepted input to task filing, source-backed
placement, the original decoder and checked generation. Worker/dev_loop keeps the
three workflow choices separate: start the task, select its fresh source, then use
its measured placement. None is an automatic consequence of receiving input.

Implements, graded by the author against merged authority:

| Authority | Grade and boundary |
| --- | --- |
| FOUNDATION-SCOPE and FOUNDATION-TRANSITION | Conforms for explicit worker orchestration and held serving cutover. |
| Spec 02 section 5 INPUT/OUTPUT; HARNESS-INPUT/MATERIAL | Conforms for the original accepted input, filed task conversion and retained checked generation; repair/sort/dispatch composition remains owed. |
| HARNESS-MARKS; Spec 06 mark allocator and native positions | Conforms for real input/mark receipts and Released/Recorded marks reads before task material or generation. |
| Spec 06 PLACE-PORT, PLACEMENT-RECORD, FIT-REFUSAL | Conforms for the actual prefix, exact selection, installed schemas, measurement, Recorded grant, original decoder confirmation and complete refusal response. |
| HARNESS-OWNERS, HARNESS-AUTHOR, HARNESS-PUMP | Conforms for retained owners, original waits and common-pump progress; general nested-wait lifecycle judgment remains owed. |
| Harness Spec 2.2; admin-harness and harness-gate contracts; HARNESS-OPERATION-CLOSE | Conforms for the exercised task/place/generation Stop and Observe joins, original exchanges and recorded-before-answer discipline. Full lifecycle and teardown composition remains owed. |

No wire field, event kind, tokenizer, component, new deadline kind or final-answer
policy is introduced. Approved unmerged proposals remain held.

## Owned transitions

`start_task` consumes the actual accepted input under the loaded Decision wait and
uses the template certificate retained from boot. It waits for the original input
filing, the complete marks read and the mark's classified filing. Read refusal
retains its exact typed code/detail and the private operation account without a
component latch. An unfiled or refused task cannot supply placeable material.

`place_task` explicitly selects the fresh filed task. Its material carries the
complete source/receipt chain. The current prefix is retained without a cut, for
either caller-supplied method. The installed owner supplies actual tool schemas,
journal facts and measurement. Only its Recorded placement produces a grant.
`generate` is a separate worker choice and consumes that private grant on the
original decoder. The returned answer must pass generation and native-position
confirmation plus OUTPUT checking. The move-owned `FoundationGeneration` keeps the
same original request and run for repair/sort composition; a correct language
refusal is retained and grants no final answer or dispatch.

Stop drains the same task filings or measurement, cancels a retained placement
grant, withdraws a wholly unsent append, or cancels and settles the original sent
generation. A delayed mark receipt and root EOF during a delayed measurement do
not release custody early. A measured fit refusal finishes its already-owned
refusal/close before the subsequent Stop is judged at rest. Observe preserves
in-flight measurement and its original wait. Fatal errors retain the full owner
and first failure for failed-run teardown.

## Verification

All Cargo was serial with `--locked`, lock gate first, and private target
`/tmp/weaver-146-boot-composition-target`.

- Harness: 1,944 passed (including 20 doctests), 149 ignored in the normal Cargo
  gate. Counts exclude duplicate summaries from fixture child re-execution.
- All 20 new generation circuits pass separately, along with all 111 prior
  Stop/Observe/input/startup circuits. The 131 exercised delegated cases leave
  18 inherited harness ignores.
- Eleven compiled protective mutations fail as intended: missing task transport,
  sleeping on internal generation progress, implicit generation, omitted source
  selection, changed placement label, omitted task/grant/generation cancellation,
  premature refusal delivery, competing refusal/Stop close, and lost cancellation
  after root EOF. All 601 workspace Rust source hashes are restored and rebuilt.
- External compiler pins accept the consuming public handoff and reject private
  input access (E0616), borrowing it to consume (E0507), Clone and Default (E0277).
- Affected harness clippy with warnings denied, formatting, initial/final lock,
  diff and three DocsMirror checks pass. Full regression and delegated suites ran
  after restoration, using the rebuilt source.

The new circuits include both no-cut methods, original source-map/UTF-8 identity,
input and mark receipt pressure, typed read/fit/output refusal, invalid decode
counts and EOF, Stop through task/mark/measure/grant/unsent/sent/completed output,
root EOF during cancellation, a preexisting refusal close, Observe during measure,
and the actual worker/dev_loop adapter with a real clock.

The actual task wait initially stalled without transport progress. Generation
also stalled when its internal Progress was mistaken for a reason to sleep for
external I/O. Both defects are fixed and independently removed by protective
controls. The initial unsent-request fixture needed real socket pressure, and the
source-map assertion was corrected to trace's external enum tags. Initial logs
are retained with the author evidence. These checks use actual forks/sockets and
recording/filing owners with scripted member/classifier/model replies and fixture
budgets. They are not a Planner/security grade or production qualification.

## Remaining work

The next join is checked generation into repair/effective emission and
SUBJECTS/OUTBOUND, followed by pair/tool execution through the same retained worker
owner. Content/plan selection, cuts, general nested lifecycle waits, Leave/SavePoint,
failed-run teardown, coupled admin/worker startup, full workflow, artifact and
limit qualification, and active serving selection remain owed. Existing serving
stays selected. This draft slice closes no full issue or epic checklist item.
