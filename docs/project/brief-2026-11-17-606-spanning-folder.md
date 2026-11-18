# Brief: a target that holds a base refuses the archive and the purge (#606)

Written by the WeaverAgent Executor on 2026-11-17, from the plan the Planner accepted
that day. One draft pull request to `main`, from `8cee0275`.

## Why

`decommission.sh` judges a folder that holds a base one way when it plans and another
way when it acts. Discovery takes each agent's folder under each base as a target: it
plans the folder with its size, counts the agent's territory as covered by it, and says
the agent's home is archived with it. The archive and the purge pass every path through
`spans_a_base` and drop a path that is a base, or holds a base, an agent's folder or a
root's own territory. So a folder that holds a base is planned as taken, archived
nowhere and purged nowhere, and the purge removes its account. The agent's territory and
home stay on disk with no archive copy.

A hand edit of the stack record reaches it, with or without an agent made inside: the
script takes the record's `agent-directory` as a base, whatever it names. The same class
has five shapes:

| Shape | How it arises |
|---|---|
| an agent's folder holds a base | the record names the folder or a path in it |
| a root's own territory holds a base | the record names a path in a custom territory |
| a root's own territory holds another's | a `territory` key names a path inside another root's custom territory |
| a home under `/home` holds a base | the record names a path in `/home/weaver-<agent>` |
| a weaver entry in `/tmp` holds a base | the record names a path in `/tmp/weaver-<agent>` |

`create-agent.sh` makes an agent inside another only after such an edit, and its base
checks pass a folder (root:root 0755) or a territory (root, 0710). The appliance reaches
none of these: its agent directory is fixed at setup (#594).

## Rulings (the Planner, 2026-11-17)

1. **A refusal after the plan.** It fails safe, and one comparison stands behind both
   discovery and the action side. Archiving the parts that hold no base was declined: it
   needs a ruling on which of two standing rules wins (an agent's folder is taken whole
   by the ruling on #578, and a base's entries beyond its agents' folders stand, the
   ruling of 2026-11-16), and it adds purge lines to a destructive step.
2. **`/etc/weaver` is judged too**: the accounts go while what they own stays, which is
   the same harm. A prefix's parts are judged only if they pass through the same
   comparison at no new surface.
3. **One draft pull request to `main`**: this brief first, the suite's litter fix as its
   own commit, then the code.

The refusal names each target that holds a base and what it holds, and says what the
operator does next, pointing at `deploy/HowToDeployANewAgent.md` section 7.
`create-agent.sh`'s guard has a test for each shape it can meet, and the layout from
before #578 passes.

## What changes

**`decommission.sh`.**
- `spanned PATH` holds the comparison `spans_a_base` held, moved whole, and prints what
  PATH is or holds: the base, an agent's folder, a root's own territory or the
  operator's home. `spans_a_base` asks `spanned` and drops its answer, so the archive,
  the purge's derivation and the refusal read one comparison.
- Once every target is known, after the `/tmp` entries are listed, `/etc/weaver`, each
  agent's folder, each root's own territory, each home under `/home` and each weaver
  entry in `/tmp` is judged. One that holds a base or another agent's folder prints a
  `REFUSED` line in the plan, naming what it holds.
- After the plan, the archive and the purge refuse, first run and rerun, before the
  archive directory is made or the purge list is read.
- A link is not judged: the archive stores a link as a link and the purge removes the
  link, never what it points to, so an agent's link in `/tmp` to a base refuses nothing.
- Unchanged: the archive and the derivation keep their filter as a second wall. The
  earlier layout's directory and the legacy log paths keep their line, "not taken whole,
  left in place", which is true, and they never join the targets.
- **A prefix's parts and the run directories are not judged.** They are derived inside
  the archive's purge list, and again in the purge's derivation, and never before the
  plan, so judging them means a third derivation or moving both, which is new surface.
  A prefix's part skipped for holding a base is neither archived nor removed, and no
  account owns it. A run directory is state on a tmpfs, purged without an archive.

**`create-agent.sh`.** Before anything is made, the record's `agent-directory` is
compared with the territory (its root's `territory` key), the folder and the account's
home of each agent whose root stands under the admin base. The folder is named as
`decommission.sh`'s `folder_bases` names it: by a key naming
`<X>/<agent>/weaver-<agent>` or a home at `<X>/<agent>/home`, read lexically, so a
folder whose territory is gone is still seen. A base equal to one of them, or under one,
refuses, naming the agent and the place. A key that stands and cannot be read refuses
too, as do a root that cannot be searched and an admin base that cannot be listed. Keys
are 0644 under a root 0755 and the base is 0755, so the operator reads them in either
mode, and no step is left pending.

**Documents.** `deploy/REDEPLOY.md` section 1 says the refusal.
`deploy/HowToDeployANewAgent.md` section 7 says what it means and how to unpick it.

**The suite's litter** (found in the clean-up after #598). In root mode the tests route
every root step to the stand-in, which passes a drop's relay directory's `mktemp`
through to the real one and has no such case for the `rm -rf` that removes it. Each
root-mode drop left a `/tmp/weaver-drop.*` directory: 34 per full run. The stand-in
passes that `rm` through as well, and the root-mode test that drops to an account
asserts that no relay directory outlives it.

## Acceptance

- Every shape refuses in all three modes, and the ruled layout, a near name and the
  earlier layout's directory holding a base do not.
- `spanned` and `spans_a_base` agree on every case.
- `create-agent.sh` refuses a base in a standing agent's folder, its territory there,
  its home there, a custom territory and a home outside its folder, and in a folder
  whose territory is gone or that only its home names. It takes a base beside or above
  one, and the layout before #578.
- An agent's link in `/tmp` to a base refuses nothing.
- Each perturbation in the pull request's body turns its test red.
- The full `test_plans.py` and workspace suites pass, with no sudo reached.

`Closes #606`.
