# Brief: the service's socket, act one's first stack

Written by the WeaverAgent Executor on 2026-11-26, on the Planner's word of 15:54 UTC
and 16:59 UTC that day, from the operator's rulings of 2026-11-26 and the four
exchanges the WeaverWeb seat filed as WeaverAgent issue 645. One branch from `main` at
`041fc448`. This stack is documents only: this brief, then the contract's text in
`weaver-admin-operator-contract` section 8 with the one pointer its Spec needs. No
binary, unit or script comes before the operator has merged it (the suite workflow,
section 7).

## Which weaver-admin is meant

Three things carry the name, and every site in this stack says which one it means.

- **The weaver-admin account** is the system account `weaver-admin`, under which the
  service runs.
- **The service** is `weaver-admin-service`, a third binary of the admin crate, which
  stands from boot under the weaver-admin account and answers on the socket this stack
  specifies.
- **The root program** is `weaver-admin`, the binary the operator runs as root and the
  sudo rule names, one fixed line per verb. It keeps that name until act two splits it.

## Why

The operator's ruling of 2026-11-26 moves the sudo rule from the admin connector to a
standing service under its own account, and strips the connector of its rule, its
groups, its read of `admin.log` and its dial of the relay. What the connector did with
those it now asks the service on one local socket. The operator chose the second of the
two shapes on 2026-11-26 at 15:48 UTC: the connector keeps its link to Weaver Web and
its protocol, and asks the service four small things. Which verbs are granted, run this
verb, the trace from this position, and the admin log's lines. A cross-process
protocol takes a state table before any code (`CLAUDE.md`, the pull request path, step
6), and both repositories build against the contract, so the contract lands first.

## Rulings this stack stands on

The operator's words of 2026-11-26, 06:00 UTC:

1. "Weaver-admin. It is a standing systemd service under its own account, not root. It
   does the pairing, then the administrative work. For root work it gets sudo rules,
   each naming one root-owned program with fixed behavior that validates its own input.
   No password, no shell."
2. "Conduits. ... Admincon is the conduit to weaver-admin. ... Admincon loses its sudo
   rule, its group memberships, its read of admin.log and its dial of the relay."
3. "Configuration. ... An edit from Weaver Web goes through weaver-admin to a fixed root
   program under sudo, which checks the input and puts the file's digest in the trace."
4. "Order. Act one: the weaver-admin account takes over the sudo rule as it stands, one
   line per verb, and the connectors are stripped."

His answer of 15:48 UTC, 77 (f):

5. "Yes, the second shape. Two conditions. Weaver-admin checks the grant itself on every
   run-this-verb request and does not trust admincon for it. The local socket's contract
   leaves room for a fifth message that carries the loop's text for loop install."

The Planner's decisions, the operator's to overrule:

6. The service is a third binary of the admin crate, `weaver-admin-service`. The root
   program keeps its name until act two.
7. systemd makes the socket, and the service is enabled as well, so it stands from boot.
8. The service runs without no-new-privs, and the Spec says why.
9. One socket path, one connection for each purpose: control (the grants and the verbs),
   the trace, and the log, each opening with one line naming its purpose and the agent.
10. Every document of the stack says which weaver-admin it means.

## The two conditions, read

**Condition 1, the grant checked on every verb.** The Planner's reading holds, and the
contract takes it whole (section 8.4).

- The grants exchange gives the connector its ceiling for Weaver Web and is never the
  service's authorization.
- On each verb ask the service reads, in that request and after the ask arrived, whether
  the machine grants that verb on that agent. In act one the read is the one the
  connector makes today, run by the weaver-admin account: `sudo -n -l` on the exact
  fixed line. Exit 0 is granted, exit 1 is not granted, and anything
  else, or no answer within 10 seconds, is a grant that cannot be read.
- sudo exits 1 for a rule it could not read as for one that does not grant, so exit 1
  means "not granted, or a rule sudo could not read". No decision is made from sudo's
  standard error, and the service's own log keeps it for each read that did not answer
  granted. That is the planning seat's decision of 2026-11-26, and the operator's to
  overrule.
- A verb not granted is refused `not_granted`, and one whose grant cannot be read is
  refused `grant_unread`. Neither runs anything, and both are the service's own words,
  never inside admin's object and never an exit code.
- Nothing sent earlier counts, on this connection or another. A grant removed between a
  grants answer and the verb is refused at the verb.
- The state table below has no path to a run that does not pass that read (invariant
  S1).

**Condition 2, room for a fifth message.** The Planner's reading holds, and the
contract takes it in a section of its own (section 8.9).

- The list of purposes is closed, and an amendment extends it. A purpose the service
  does not know is refused `purpose_unknown`, with the purposes it knows, which a newer
  connector reads as "not here yet".
- The opening line's version is what moves. A version above the service's own is
  refused `version_unknown`, with the versions it speaks. A service answers every
  version at or below its own as that version was written, so the four exchanges do not
  change when the fifth arrives.
- The framing is lines, with one form reserved: a header line naming `length`, then
  exactly that many raw bytes. The reserved message carries a text up to 256 KiB, the
  bound admin's judge of the loop folder holds for the wire file today, with its sha256.
  No message of act one uses the form, and a header naming `length` is refused
  `ask_malformed`.
- The fifth message is the loop's third act's. The text goes through the service to a
  fixed root program under sudo, which checks the input and puts the file's digest in
  the trace. The contract says that much and no more.

## The brief's leans, for the Planner's grade

These are choices the notes leave open, or one where my reading departs from the
Planner's. Each is stated in the contract as written here, and each is one sentence to
change.

1. **One run at a time per agent, not per machine.** The Planner's note says "One run at
   a time on the machine." I lean per agent, and I think the machine-wide rule is the
   wrong one.
   - The root program's invocation lock is per agent (`weaver-admin-Spec` section 3,
     the lock rule), and the connector holds one invocation slot per agent today (issue
     645, section 2, `admin_con.rs`:2015-2045).
   - Under a machine-wide rule, one agent's `show` would wait behind another agent's
     load, whose bound is 900 seconds by default.
   - On a one-agent appliance the two rules are the same.
   - **The two overlaps admin admits are kept** (`weaver-admin-operator-contract`
     section 4.1): a `show` starts beside any run, since admin's `show` alone reads
     beside a holder, and a `force-unload` asked while an `unload` of the same agent
     runs is started beside it. Every other second ask is refused `busy`, naming the
     run that holds the agent.
2. **The service answers the opening line before any purpose's exchange begins:** one
   `open` line, or one `refused` line and a close, at most 4096 bytes and within 10
   seconds of the opening line, the log's judging and the relay's dial included, each
   cut off at 8 seconds so a refusal arrives in time. A refusal of the opening is then a
   typed line on every purpose. On a trace connection the service dials the relay before
   it answers and sends `open` within 1 second of the dial's connect. Where no relay
   stands, the dial fails or that second passes, it refuses the opening
   `relay_unreachable`. The connector sends its request within 1 second of reading
   `open`, inside the relay's 5 seconds from its accept. The trace's own refusal, a
   close before the header, stays the relay's, after `open`, and is never a relay that
   was not there.
3. **The peer admitted is the admin connector's account alone,** by its uid from
   `SO_PEERCRED`, checked at accept. Root is refused too. The operator at a root shell
   runs the root program directly and never needs the socket. A refused peer reads
   nothing, and the connection closes.
4. **A run carries a run id,** 16 hex digits drawn by the service. A `started` line
   names it once the root program is running, and the final line names it again. A new
   control connection asks `run` and learns which runs stand and how the last one
   ended, by that id. A `show` is never listed in `standing` and never kept as `ended`,
   so it never hides the outcome of a verb asked before a drop.
5. **The principal is recorded in the service's own log,** one line per run, with the
   run id, the agent, the verb and the outcome. `admin.log` and the trace carry the
   uid sudo reports, which is the weaver-admin account's. The principal is never an
   argument of the root program.
6. **The log flag is judged from the line alone.** A line whose caller uid (an
   invocation's line) or peer uid (the relay's line) is the weaver-admin account's
   is marked `through_service`. In act one the service runs the root program only on a
   control connection's ask, and it is the relay's one declared reader, so that uid
   names exactly the acts that came through the socket. An act that gives the service
   a run of its own, the pairing among them, amends this rule.
7. **A restart of the service forgets its runs.** A root program the service started
   runs to its end whatever becomes of the service. That is a requirement on the
   service's install, which act one's code stack meets: the service's stop signals its
   own process alone, since sudo and the root program stand in the service's
   containment until a load moves into the agent's scope. The root program's own invocation
   lock refuses a second verb meanwhile, inside admin's object, and the outcome is in
   `admin.log` and the next `show`. Each start of the service draws an `instance`,
   which every `open` and every `run` answer names. Under another `instance` the
   connector reads its ask as unknown, never started or forgotten, and `show` settles
   it. Under the same `instance`, a run neither standing nor `ended` did not run, where
   the connector asked no other verb meanwhile. A connector holding no run id reads its
   run as a standing run of its verb, or an `ended` of its verb whose `run` is not the
   last id it learned had ended, and otherwise as not run.
8. **The socket's path is `/run/weaver-admin.sock`,** owned by the weaver-admin
   account, grouped to the admin connector's own group, mode `0660`, made by systemd.
   That group holds no member but the connector's account, so the mode admits the
   connector, and `SO_PEERCRED` checks it again.
9. **Connections are bounded per agent.** One connector serves every agent, so one
   machine-wide bound would let watched agents crowd out a control connection. Each
   agent holds at most one log connection, the newest replacing the older when it
   opens, one trace connection the relay reads for, replaced only when the relay admits
   a newer request, and 8 control connections, under 1024 at the service in all.
10. **A refusal of an ask's kind leaves the connection standing.** `ask_unknown` and
   `verb_unknown` answer and stay, so a newer connector probing a later ask or verb
   reads "not here yet". A malformed line and a line sent over an outstanding ask
   close it.

## The state table

The protocol has two machines: one per connection, and one per agent at the service.
The contract's section 8.8 carries the same table and is its authority.
The connector causes the C events, the service the V events, and the machine the M
events.

### Machine K, the connection

| State | What holds |
| --- | --- |
| K0 | Accepted. The peer's uid is read |
| K1 | Admitted, waiting for the opening line |
| K2 | Control, no ask outstanding |
| K3 | Control, a grants ask outstanding |
| K4 | Control, a verb ask outstanding, its grant being read |
| K5 | Control, a verb ask outstanding, its run started |
| K6 | Control, a run ask outstanding |
| K7 | Trace, after `open`: the relay's wire |
| K8 | Log, after `open`: lines as appended |
| K9 | Closed |

| State | Event | Next, and what is sent |
| --- | --- | --- |
| K0 | V: the peer's uid is the connector's | K1 |
| K0 | V: any other uid | K9, nothing sent, the service's log names the uid |
| K0 | V: a bound of section 8.1 is met | K9, `refused` `full` |
| K1 | C: a control or log opening line that judges | K2 or K8 by its purpose, `open` sent |
| K1 | C: a trace opening line that judges, and the service's dial of the agent's relay connects | K7, `open` sent within 1 second of the connect |
| K1 | C: a trace opening line that judges, where no relay stands, the service's dial of it fails, or `open` cannot be sent within 1 second of the connect | K9, `refused` `relay_unreachable`, a dialed connection closed |
| K1 | C: close, while the service dials the relay | K9. The dialed connection is closed, before it sends the relay any byte |
| K1 | C: an unknown version or purpose, a refused name, or an unread log | K9, `refused` with its word |
| K1 | C: a line past 4096 bytes, one that does not parse, or a version that is not a positive integer | K9, `refused` `opening_malformed` |
| K1 | V: a newer trace or log connection of the agent opens | no change. Neither rule of section 8.1 reaches a connection whose opening line is not yet judged |
| K1 | V: 10 seconds pass with no line | K9, `refused` `opening_late` |
| K2 | C: `grants` | K3 |
| K2 | C: `verb` naming one of the eight, with a principal of its shape | K4. The grant is read now |
| K2 | C: `verb` naming another verb | K2, `refused` `verb_unknown` |
| K2 | C: `run` | K6 |
| K2 | C: an unknown ask | K2, `refused` `ask_unknown` |
| K2 | C: a line that does not parse, past 4096 bytes, naming `length`, or a principal of another shape | K9, `refused` `ask_malformed` |
| K2 | C: close | K9 |
| K3 | V: the eight grants read | K2, `grants` naming the granted verbs |
| K3 | V: a read fails, or 10 seconds pass | K2, `refused` `grants_unread` |
| K3 | C: close | K9 |
| K4 | V: granted, and machine A admits the run | K5, `started` with the run id |
| K4 | V: not granted | K2, `refused` `not_granted` |
| K4 | V: the grant cannot be read within 10 seconds | K2, `refused` `grant_unread` |
| K4 | V: granted, and machine A refuses the run | K2, `refused` `busy` naming the run that holds the agent |
| K4 | V: granted, and the root program does not start | K2, `refused` `not_started` |
| K4 | C: close, before the read answers | K9. Nothing starts |
| K4 | C: close, after the read answers granted | K9. The start goes on as if the connection stood, and a run started, a `show` excepted, is kept in machine A |
| K5 | V: the root program ends with exit 0 or 1 and one object | K2, `ran` |
| K5 | V: it ends any other way | K2, `fault` |
| K5 | C: close | K9. The run goes on, kept in machine A unless it is a `show` |
| K6 | V: machine A read | K2, `run` |
| K6 | C: close | K9 |
| K2 to K6 | C: a line while an ask is outstanding | K9, `refused` `ask_outstanding`. A run started goes on |
| K7 | the relay: a close before any header byte | K9, the connector's side closed. It is the relay's refusal of the position |
| K7 | C closes, or the relay closes after a header byte | K9, the other side closed |
| K7 | the relay: it admits a newer trace request of the agent and ends this connection | K9, the connector's side closed. A newer trace connection's opening alone closes nothing |
| K8 | V: a line appended | K8, `log` with `line` or `text`, or the `oversized` marker past 8 KiB |
| K8 | V: the file truncated or replaced | K8, the `discontinuity` marker, the file read from its start |
| K8 | C: any byte | K9, `refused` `ask_malformed` |
| K8 | V: a newer log connection of the agent is opened | K9, closed |
| K8 | V: 1 MiB stands unsent | K9, `refused` `log_behind` where the connection takes it |
| K8 | C: close | K9 |
| any | M: the sudo rule is edited | no change. The next verb ask reads the rule as it stands then |

### Machine A, the agent's runs at the service

| State | What holds |
| --- | --- |
| A0 | No run of the agent stands. The last run that ended, a `show` excepted, is kept, or none since the service started |
| A1 | One run stands |
| A2 | An `unload` and a `force-unload` beside it stand |

| State | Event | Next |
| --- | --- | --- |
| any | a `show` ask, granted | no change. The `show` starts beside whatever stands, and is not a run this machine holds |
| any | a `show` ends | no change. Its final line goes to its own connection alone, never kept as the last ended |
| A0 | a verb ask other than `show`, granted | A1, holding that run |
| A1 | a `force-unload` ask, granted, while the run A1 holds is an `unload` | A2, holding both |
| A1 | any other verb ask but `show`, granted | A1, that ask refused `busy` |
| A1 | the run A1 holds ends | A0, its final line kept as the last ended |
| A2 | another verb ask but `show`, granted | A2, that ask refused `busy` |
| A2 | the `unload` or the `force-unload` ends | A1, holding the one still standing, the ended one's final line kept as the last ended |
| any | the asking connection closes | no change |
| any | the service restarts | A0 with no run kept, under a new `instance`. A root program it started runs on, under the root program's own lock, where the install meets section 8.4's requirement |

### The invariants

- **S1.** A root program starts only from K4, after a read of that verb's grant on that
  agent, made in that request after its ask arrived, answering granted. No path from K2
  reaches K5 without it.
- **S2.** Nothing enters S1's read from a grants answer, an earlier ask, or another
  connection.
- **S3.** The principal enters no decision: not S1's read, not machine A, and not the
  root program's arguments.
- **S4.** A run's end does not depend on its connection. A close before the grant's
  read answers starts nothing. From the moment the root program is started, the run
  goes on to its end, whether or not `started` reached the connector, and is kept unless
  it is a `show`.
- **S5.** At most one run of an agent stands, except a `show`, which starts beside any
  run, and the `force-unload` beside an `unload` of A2.
- **S6.** Every ask gets exactly one final line, or the connection closes. A verb's
  `started` comes before its final line.
- **S7.** After `open` on a trace connection, the bytes each way are the relay's,
  unchanged. `open` is sent only once the relay's dial has connected, and within 1
  second of the connect, so a close there before any header byte is the relay's refusal
  of the position alone.
- **S8.** A refusal of the service's own is a `refused` line, never inside the root
  program's object and never an exit code. A refusal of the root program's is inside its
  object, at exit 1, carried untouched.

## The children

| Child | Branch | What it holds |
| --- | --- | --- |
| 1 | `act/the-admin-socket-contract-1-the-brief` | This brief |
| 2 | `act/the-admin-socket-contract-2-the-contract` | `weaver-admin-operator-contract` section 8, the socket, with its limits, the state table, the reserved fifth message and what it does not carry. Section 9 is the old section 8, with this act added to it. One pointer in `weaver-admin-Spec` section 2, naming the service and why it runs without no-new-privs |

The base is `act/the-admin-socket-contract` at `041fc448`. It does not stand on the
loop's second act.

## Not in this stack

- No binary, no unit, no script, and no change to `create-agent.sh`. They are the next
  stack, on a brief, after this one merges.
- The sentences of sections 0 to 3 of the contract and of `weaver-admin-Spec` section 2
  that give the connector the sudo rule and the trace door stay true until that code
  stack lands, and that stack changes them.
- The gate's door, the names refused, the rule's caller, the per-part read access, each
  connector's key in a file of its own, and the layout.

## Acceptance, and where each point stands

| Point | Where |
| --- | --- |
| The operator's condition 1 | contract section 8.4, "The grant is read in the request", and invariant S1 of section 8.8 |
| The operator's condition 2 | contract section 8.9, and the framing paragraph of section 8.2 |
| Every limit with a number and a word | contract section 8.7, one table |
| A fake service from the contract alone | contract sections 8.2 to 8.6 give every line's shape and each refusal's cause. Section 8.10 lists what the fake for issue 645 section 7 must do |
| The other seat's word | carried by the Planner, before the push word |
| No person's account, host, address, or path of a site | the product's own names alone: the three accounts, the socket's path, the root program |
