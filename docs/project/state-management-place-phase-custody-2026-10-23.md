# Placement phase and returned custody followups

Date: 2026-10-23. Branch: code/state-place-phase-custody. Exact parent #277
65d3b111736e5fea15513ab8b08f5384a03bebf0. This record advances issue #146;
it is not a Spec amendment, serving activation or production qualification.

Implements: Spec 06 CUT-WAIT, STAGED-APPEND, native mark completion and filing
receipt custody; HARNESS-PLACE, HARNESS-RECORD-LIFETIME and the existing Stop,
Leave and Failed account boundaries. Author assessment: conforms within the
dormant owner. The slice answers #273's eight followup rows, with the original
redundant/overlapping controls kept separate from new behavioral evidence.

## Returned evidence and live completion

PlaceAccount captures the context's actual phase when its owner is extracted.
A Failed or incomplete reload retains borrowed prefix, tail and Staged evidence
without returning retry inputs or carry. A completed no-mutation Releasing
account may return its retained inputs as evidence, but the retiring run cannot
admit generation. Bounded extraction clears the engine's carry identity.

A live cancelled cut now returns CancelledStaged with the actual Staged beside
its original account. The capability is explicit and the account retains its
raw/terminal/Recorded facts. Disposing the account cannot hide that carry.
Ordinary Staged, no-mutation Cancelled and Refused remain distinct. All current
consumers are dormant owner tests; no serving caller is claimed migrated.

Normal reload and native completion extraction require Entered. In particular,
review found that take_reload's live check alone allowed Releasing. The added
phase guard runs before consuming completion, removing its owner or disarming
Cut. A real completed Flush during Leave tests unchanged owner pointer, engine
identity, Cut/Leave bounds and trace count before bounded account extraction.
Failed still preserves the existing first-cause refusal before these checks.

## Retried source receipts and ownership shape

Retry admission verifies each retained consumed source against its actual
Taken ledger slot, context identity, ordered tail index and Held material's
immutable committed source. After all admission checks pass, it releases only
that redundant consumer subscription. The Held unit retains the real receipt;
selected filing debt is unchanged. A failure returns its actual lease and
original inputs with the existing internal fault. There is no new filing,
classification, registration, cloned lease or reconstructed receipt.

ReturnedReloadInputs fields are private. Borrowing accessors expose evidence;
into_retry remains the only consuming conversion and preserves explicit source
rejection. Live reload prefix/tail accessors state the retained-owner invariant
once, with taking confined to settled extraction. A returned prefix is the last
checked, possibly re-prepared prefix at that boundary, not necessarily the
caller's initial value.

Entered native completion moves its exact next scopes into the engine. Only a
later cancelled-account extraction copies them for evidence. The native owner
slot prevents competing scope changes before extraction; ordinary success no
longer clones. Actual barrier debt is checked before and immediately after
cancelled extraction, followed by a real next-turn generation.

## Validation and limits

Source review covers eleven Rust paths and the explicit two-file phase fix.
Five new actual-owner tests and strengthened reload/native tests pass. The
full harness suite passes 1,738 tests with 18 inherited ignored cases, and
trace passes 160. The phase, reload-return and native-return groups pass
5, 5 and 4 tests. Harness all-target Clippy, formatting, lock and diff checks
pass. The full suites precede only the final test-only two-source strengthening;
its five reload-return tests and lint/format checks pass afterward.

Ten unique behavioral mutations compile and fail their named tests, followed
by exact restoration and passing tests. The original run had nine failures
and one survivor: a wrong index selected a Held unit with no filed source, so
that earlier refusal masked the exact-source comparison. The final fixture
generates and files two distinct real sources, swaps their Held units while
keeping both original Taken leases and ordered indexes, and proves refusal.
Both source equality and consumed-subscription cleanup mutations then fail.
The original survivor and failed author checks remain in the evidence.

The separate four-field E0616 compiler pin rejects direct access to prefix,
original_tail, resolved and rejected; it is not counted as a behavioral
mutation. Its exact restored named test passes on the final source.
These are author checks, not a Planner grade or production qualification.

The Releasing native fixture reaches its separate finish after every real
position is Recorded, so it reaches the adoption phase guard. Failed reaches
the existing first-failure check first and is an overlapping control. Swapping two
retained filed Held sources is a defensive corruption of otherwise genuine owners;
it is not a claim of public race reachability. The lease-count assertion runs
immediately after admission, before later tail disposal could hide a leak.

Original N1/N2 remain implied by done/Recorded/incomplete writers; M3/M6 remain
implied by completeness and author guards. Removing only the ordinary native
success phase check still has no reachable independent case because teardown
revokes work first. No synthetic done/cancelled/phase states claim otherwise.
The separate field-access compiler rejection must not count as a behavioral
mutation. The parent settled-terminal Cancel correction remains intact.

Remaining #269 rows 3/7/8, Q2/Q7 redundancy, Q8/Q9 overlap and Q14's separate
redundant/untested claim remain tracked. #274/#275 followups, full Failed and
boot/worker composition, serve-side selection, qualified artifacts and measured
coupled cutover remain owed. Approved but unmerged #260/#263/#268 are unchanged;
#276's author-return proposal remains unelected/unmerged and unimplemented.
Only the operator merges and deploys. All new PRs start as drafts, and #146
remains open.
