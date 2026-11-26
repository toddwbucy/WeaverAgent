# The identity run on an agent appliance

This is the procedure for the run act 2's brief names as the gate on the compiled loop
(`docs/project/brief-2026-11-26-the-loop-process.md` section 8, operator's ruling 73 f):
today's loop written as wires, run on an agent, with a record identical to the compiled
loop's by `weaver-harness-loop-contract` section 6. The build machine holds no agent, so
the run is made on an agent appliance with its cards and the declared model, by root,
who is the operator there. Nothing here runs on the build machine but the comparison's
and the plan's own tests.

The tools beside this file:

- `run-arm.sh`: one arm, made, driven, copied out and taken down. It plans by default
  and acts with `--apply`.
- `take-down.sh`: one agent taken off the appliance by the runbook's section 7
  (`deploy/HowToDeployANewAgent.md`), archived as root first. It leaves the stack, its
  record and the machine's own accounts standing, the connector accounts among them, so
  every arm runs on the one install. It plans by default and acts with `--apply`.
- `compare.py`: the comparison over the four arms, standard library alone.
- `fullness.py`: the reading `run-arm.sh` takes after each turn of load 1, to know when
  the next runs under pressure. It waits for the answered turn's close in the record,
  which lags the answer, and fails the arm where the close does not come in 30 seconds.
- `test_compare.py`, `test_fullness.py`, `test_run_arm.py` and `test_take_down.py`:
  their tests, the first of the two negative controls among them. They test the
  comparison, the reading, and the two scripts' plans and refusals. **Every `--apply` step of the two scripts is unmeasured
  until this run**: read each plan before its first apply, and keep every console.

## 1. Before the run

1. **The stack is built from act 2's head**: the worker, the launcher
   (`weaver-loop-launch`) and the runner's files. Install it with the stack's own
   scripts, `deploy/bootstrap-stack.sh --install` or `deploy/update-stack.sh`, or from a
   carry set that holds the same build.
2. **The tree on the appliance.** Every command here runs from the root of a checkout of
   the same commit as the stack, laid on the appliance by root: `git archive <commit>`
   on a machine that holds the repository, copied over and unpacked. `run-arm.sh` takes
   the `deploy/` folder beside its own (`--deploy` names another, such as a deployed
   copy, which holds what `deploy/deployed-copy.txt` lists), and finds `take-down.sh`
   and `fullness.py` beside itself.
3. **Read the grant back.** The stack lays `<prefix>/lib/weaver-loop/` with
   `runner.py`, `weaver_board.py` and `grant`. The grant digests the interpreter and
   names the rest as paths. Read it back:

   ```sh
   cat /opt/weaver/lib/weaver-loop/grant
   sha256sum /opt/weaver/lib/weaver-loop/runner.py /opt/weaver/lib/weaver-loop/weaver_board.py
   sha256sum "$(sed -n 's/^interpreter [0-9a-f]\{64\} //p' /opt/weaver/lib/weaver-loop/grant)"
   ```

   The interpreter's digest in the grant must be the interpreter's sha256. Write the
   grant and all three sha256 values into the run's notes.
4. **The appliance holds no agent.** `ls -A /etc/weaver/admin` lists nothing.
   `deploy/decommission.sh` is not used anywhere in this run: it would remove the
   stack, its record and every `weaver-` account.
5. **The two wire files.** The wires arm runs today's wire file,
   `crates/weaver-harness/tests/fixtures/loop-wires-today.py`, from the same commit as
   the stack. The negative arm runs the same file with one byte of its prompt text
   changed:

   ```sh
   cp loop-wires-today.py wires-today.py
   sed 's/^SYSTEM_PROMPT = "You are a careful/SYSTEM_PROMPT = "You are a cAreful/' \
       wires-today.py >wires-negative.py
   cmp -l wires-today.py wires-negative.py   # exactly one line: one byte differs
   sha256sum wires-today.py wires-negative.py
   ```

   Write both sha256 values into the run's notes. The first must equal the sha256 the
   act's reports give for the fixture.

## 2. The owner checks no build-machine test reaches

Admin refuses a `loop/` or a `wires.py` that is not root's. No test on the build machine
can lay a file owned by another account, so these are root's steps here, on a throwaway
agent, before the run:

```sh
R=<the run's folder, made by root for the whole run>
deploy/create-agent.sh probe --engine sqlite --artifact <model> --root-operator --apply
install -d -o root -g weaver-probe -m 0750 /etc/weaver/probe/loop
install -o root -g weaver-probe -m 0640 wires-today.py /etc/weaver/probe/loop/wires.py
/opt/weaver/bin/weaver-admin validate probe          # {"kind":"validated"}
chown nobody /etc/weaver/probe/loop
/opt/weaver/bin/weaver-admin validate probe          # refused: the loop folder is not root's
chown root /etc/weaver/probe/loop
chown nobody /etc/weaver/probe/loop/wires.py
/opt/weaver/bin/weaver-admin validate probe          # refused: the wire file is not root's
chown root /etc/weaver/probe/loop/wires.py
/opt/weaver/bin/weaver-admin validate probe          # {"kind":"validated"} again
tools/identity-run/take-down.sh --agent probe --archive $R/probe-archive             # the plan
tools/identity-run/take-down.sh --agent probe --archive $R/probe-archive --apply 2>&1 | tee $R/probe-take-down.console
```

Keep each answer. Each refusal names the piece and that it is not root's.

## 3. The four runs

Run each arm in turn, under one agent name, with the same model and the same options.
Each is made fresh and taken down before the next. Keep every console:

```sh
# R as in section 2
tools/identity-run/run-arm.sh --arm compiled --agent idrun --artifact <model> --out $R/compiled-1 \
    --apply 2>&1 | tee $R/compiled-1.console
tools/identity-run/run-arm.sh --arm compiled --agent idrun --artifact <model> --out $R/compiled-2 \
    --apply 2>&1 | tee $R/compiled-2.console
tools/identity-run/run-arm.sh --arm wires --wires wires-today.py --agent idrun --artifact <model> \
    --out $R/wires --apply 2>&1 | tee $R/wires.console
tools/identity-run/run-arm.sh --arm negative --wires wires-negative.py --agent idrun --artifact <model> \
    --out $R/negative --apply 2>&1 | tee $R/negative.console
```

Run each without `--apply` first and read the plan. Each arm ends with its agent
archived into `<arm>/archive` and taken down by `take-down.sh`, the stack left standing.

**Not measured before this run:**

- Whether the gate admits uid 0. The seeding turn needs it (`deploy/turn.py`), so
  `run-arm.sh` adds uid 0 to the access rule in every arm. If the gate refuses the
  seeding turn, the arm stops at load 4. Report that: the paths with a prompt seated
  then go unmeasured.
- How many turns pressure takes. It depends on the model's answers. The script bounds
  it at 40.

## 4. While the wires arm runs, and after it

These are the measurements the worker's start leaves to a real load (act 2, child 5):

- **The runner under the worker, as the agent's account:** each `steps/<n>-processes.out`
  in the wires arm shows a `python3` process running `runner.py`. Its parent is the
  worker, and its user is the agent's account, `weaver-idrun`.
- **The load names the loop:** each of the wires arm's six `load` events in
  `trace.ndjson` has `"composer":{"binary":"worker","file":"/etc/weaver/idrun/loop/wires.py","sha256":<the wire file's>}`.
  `compare.py` checks it.
- **The runner's end lines:** `worker.log` holds one `{"loop":"end","passes":...}` line
  for each load. Their passes sum to the turns the arm sent, without the seeding turn.
  `compare.py` checks it.
- **The compiled arms hold no runner:** no `runner.py` process in their process tables,
  and no end line in their `worker.log`.

## 5. The comparison

```sh
python3 -I tools/identity-run/compare.py run $R/compiled-1 $R/compiled-2 $R/wires $R/negative \
    | tee $R/result.json
echo "status $?"
```

| Status | Verdict | What it means | What to do |
| --- | --- | --- | --- |
| 0 | `identical` | The control arms are equal and the wires arm equals them. The loads name the laid file, the runner served every pass, the images agree where the control's do, and the negative control turned red. | Report it: the gate is met. |
| 1 | `fails` | A check did not hold. `reason` and `difference` name the first that failed. | Report it. Do not re-run to get another answer. |
| 2 | `control-differs` | The two compiled arms differ, so generation does not repeat bit for bit here, and the wires are not compared. | Report it, with the difference. |
| 3 | `unclean` | An arm holds a timing path outside the comparison: a recorder pressure fault, a state seam that died, or the recorder's queue at its mark. | Run that arm again, at most three times. Then the run fails. |
| 4 | `unreadable` | An arm's files are missing or malformed. | Read `reason`, then fix the copy or run the arm again. |

The saved images are read one save point at a time. Where the two compiled arms' images
of a save point are equal, the wires arm's must be. Where they differ, that save point is
not compared. `result.json`'s `images` names the save points compared and those not
compared, by their order in the record.

## 6. What comes back

Send back:

- `result.json` and its status
- the four arm folders, without `archive/`, each with its `SHA256SUMS`
- the four consoles
- the notes: the grant read back, both wire files' sha256, and the answers from section 2.

The run's evidence is a deposit. The repository carries only the result note.

## 7. What the run does not answer

- Whether the loop's turns are good. That is evaluation.
- Whether the loop is functional on other paths. That is act 3's `check-loop`.
- The worker's behaviour under a refused launch or a broken loop on the appliance. The
  build machine's tests drive those against the real launcher and runner.
