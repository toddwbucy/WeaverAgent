# Audit: the shipped dependencies against the rule of #577

**Status:** AUDIT, 2026-09-28. A dated reading rather than a member of the
document set, and **nothing here is decided until the operator rules on it.**
It is epic #665's report and nothing else: no code, manifest, Spec or process
file changes with it, and every judgment call is marked for the operator.

**Base:** `e39f1339` on main. Every dependency below was read from `Cargo.lock`
with `cargo tree --locked --offline`, the source with `git grep`, and the binaries
from a release build of the in-scope members at that commit. Nothing is taken from a
manifest's own account of itself where the lock, the source or the binary could be
asked, and the last section tables how each claim was checked.

## The rule, and the hypothesis it tests

The operator's rule of 2026-09-23, recorded on #577: nothing is compiled into a
binary unless operations require it, and a library pulled in for one function
is a vulnerability import. The epic's hypothesis is that this program's shape,
Unix sockets, no async runtime, no network stack and no web framework, keeps
the surface small and mostly justified. This report measures that.

**Three readings, because "compiled into a binary" names two surfaces.** The build
compiles a closure of crates for each binary, and every one of them is code the build
trusts, build scripts included. The linker then keeps in the binary only what the
binary's own code references, so what runs is narrower than what was compiled. The
report gives each crate's compiled closure alone, each shipped binary's compiled closure
as the deploy's one workspace build resolves it, and, for the crates the calls turn on,
whether any of their code is present in the built binary.

- **Compiled, each crate alone:** `cargo tree -p <crate> -e normal,no-proc-macro`, the
  crates reached over normal edges with proc-macro crates left out, at the manifest's
  default features and then at each feature the deploy names.
- **Compiled, per package as the deploy builds it:** the package's subtree of one `cargo
  tree --workspace` at the deploy's feature string, where features are unified across
  members. This reading is per package and not per binary: a feature activates its
  optional dependency for the whole package, and cargo compiles a package's resolved
  dependencies whichever of its binaries is built, so both binaries of a package share
  one figure. `weaver-harness` is the one in-scope package that ships two, `worker` and
  `pyworker`.
- **Present in the binary, per binary:** a release build at the base with the deploy's
  member features, and a census of each binary's demangled symbols by crate. A crate
  that contributes only macros, constants or code inlined into its caller leaves no
  symbol, and `hashbrown`, `memchr` and `cfg-if` share names with the standard library's
  own dependencies, so presence is read only for crates where neither applies. This is
  the reading that tells two binaries of one package apart: `pyo3` is referenced only
  under the harness's `src/bin/pyworker/`, and it is compiled for both of the package's
  binaries and present in `pyworker` alone.

**Why the basis changed across this report's readings.** Earlier readings counted
`cargo tree -e normal`, which carries proc-macro crates that run on the host and link
into nothing, and then called the proc-macro-free tree the linked closure, which it is
not: it is what the build compiles. Every figure here is re-derived on the bases above
rather than adjusted. Counts are by name and version, with the by-name figure beside
them where the two differ.

**The host-side set, named once and in no count below.** It is derived per crate at
the features the deploy builds as `cargo tree -e normal,build` less `cargo tree -e
normal,no-proc-macro`, unioned across the ten crates: twenty-two crates by name and
version, twenty-one by name, in two parts. None of them links into a shipped binary.

- **The proc-macro closure, thirteen, twelve by name.** Every crate that derives
  `serde` compiles `serde_derive` with `syn` 3.0, `quote`, `proc-macro2` and
  `unicode-ident`. The `pyworker` feature adds `pyo3-macros`, `pyo3-macros-backend`,
  `pyo3-build-config`, `heck`, `indoc`, `target-lexicon` and a second `syn` at 2.0,
  and `postgres` adds `async-trait`.
- **The build-dependency set, nine, nine by name**, the crates a build script runs on:
  `cc`, `pkg-config`, `vcpkg`, `shlex` and `find-msvc-tools` for `libsqlite3-sys`,
  the last two through `cc`, `cfg_aliases` for `nix`, `autocfg` for `memoffset`,
  `version_check` for `generic-array`, and `rustversion` for `indoc`.


**The measure, up front.** At the manifests' default features the hypothesis holds for
the external crates: every one of the twenty-four direct external dependencies across
the ten crates is referenced by its crate's shipped source, the ten share one
serialization family, four take `nix` for the socket and process calls, two take a
hash, one takes SQLite, one takes a YAML parser, one takes a tensor reader, and the pure
member takes nothing, thirty-six compiled crates in all, thirty-three by name. **It does
not hold for two internal edges**: `weaver-diagnostic` depends on `weaver-traits` and
`weaver-state` on `weaver-types`, and neither crate's source or tests reference the
crate it depends on. **At the features the deploy builds, it does not hold for one
crate.** `weaver-state`'s `postgres` feature, which `deploy/update-stack.sh` carries,
brings an async runtime and a network client, sixty-nine compiled crates in place of
nineteen, and `weaver-harness`'s `pyworker` feature links the Python interpreter into
`pyworker`. Both are elections the documents made and neither is hidden, and what the
operator decides about them is the report's largest call.

## Out of scope, by name

`weaver-spu` is the olympus lane and is not read here. `weaver-web` left the
repository on 2026-09-26 for `WeaverTools_Project/weaver-web/` and is not read
here. Neither is skipped silently.

## What each crate ships alone, at its default features

The direct normal dependencies, the features they are taken with, what each does in the
crate, and the crate's compiled closure. Every workspace crate a package depends on
directly, as `cargo tree -e normal --depth 1` shows it, is named in its row, and none is
counted as external. Two of those internal edges are referenced by nothing, which is
call **[9]**.

| Crate | Direct external dependencies | What for | Compiled | Call |
|---|---|---|---|---|
| `weaver-traits` | `serde` (derive), depends on no workspace crate | the message model's wire derives | 2 | none |
| `weaver-types` | `serde` (derive), `serde_json` (raw_value), `serde_yaml_ng` behind the `config` feature, depends on `weaver-traits` | the wire types, the record's raw boxes, the declaration parse | 6 | **[1]** the YAML parser is one function's |
| `weaver-trace` | `serde` (derive), `serde_json` (raw_value), depends on no workspace crate | the record's line encoding | 6 | none |
| `weaver-diagnostic` | `serde` (derive), `serde_json` (raw_value), depends on `weaver-traits` | the diagnostic trace's encoding | 6 | none |
| `weaver-harness` | `serde_json`, `nix` (socket, fs, process, uio, user, poll, signal), `pyo3` behind `pyworker`, depends on `weaver-traits`, `weaver-types`, `weaver-trace`, `weaver-diagnostic` | the sockets and the fork, the Python loop | 11 | **[2]** `pyworker`, and **[3]** the `nix` feature set |
| `weaver-gate` | `serde_json`, `nix` (socket, fs, process, user, poll, signal), depends on `weaver-types` | the gate socket and its peer checks | 11 | **[3]** |
| `weaver-admin` | `serde_json`, `nix` (socket, fs, process, uio, user), `sha2`, depends on `weaver-types` with `config` | the coordination socket, the inventory's digests | 24 | **[4]** `sha2` is two functions' |
| `weaver-state` | `serde_json` (raw_value), `nix` (socket, fs, uio, user, poll), `rusqlite` (bundled) behind `sqlite`, on by default, `postgres` behind `postgres`, depends on `weaver-types` | the member's socket, the two store engines | 19 | **[5]** `postgres`, **[6]** `rusqlite` bundled |
| `weaver-analysis` | `serde` (derive), `serde_json` (raw_value), `safetensors`, `sha2`, depends on no workspace crate | the record reader, the residual columns, the capture digests | 19 | **[7]** `safetensors` |
| `weaver-internal` | none, and depends on no workspace crate | the pure member, by its own manifest instrument | 0 | none |


**What `nix` is used for, read from the source against what each feature gates.** A
feature named in a manifest is code compiled whether or not the crate calls it, so the
reading below is per crate and per feature: the calls the crate makes that the feature
gates in `nix` 0.31.3, or **not reached** where the feature is named and no call the
crate makes needs it, **tests only** where the only call is in a test, or a dash where
the crate does not name it. It was checked by the grep and then by the compiler, in a
throwaway checkout, for every named feature of every crate, two ways: `cargo check` for
the shipped code and `cargo check --tests` for the tests. A feature reads reached in the
shipped binary where the first check fails without it, tests only where the first builds
and the second fails, and not reached where both build. Two of the forty-six checks are
uninformative rather than clean: `signal` implies `process` in `nix`'s own manifest, so
dropping `process` from the harness or the gate, which both name `signal`, removes
nothing, and their `process` reading rests on the calls alone. The reading counts only
what the features gate, so `errno`, the `Signal` and `OFlag` types, and the raw `libc`
calls the harness and admin make through the re-export, which no feature gates, are not
in it. Note that `sendmsg`, `recvmsg` and the control-message types sit in the socket
module but behind `uio`, which is why two crates reach `uio` through the socket and one
does not.

| Crate | `socket` | `fs` | `uio` | `user` | `poll` | `process` | `signal` |
|---|---|---|---|---|---|---|---|
| `weaver-harness` | `socket`, `socketpair`, `bind`, `listen`, `accept4`, `send`, `recv`, `getsockopt` | `fcntl`, `umask`, `pipe2` | `sendmsg`, `recvmsg`, `ControlMessage` | **tests only**, `getuid` in `tests/service.rs` | `poll` | `fork`, `Pid`, `waitpid` | `kill` |
| `weaver-gate` | `socketpair`, `send`, `recv`, `getsockopt` | `fcntl`, `umask` | - | `getuid`, `User` | `poll` | `set_dumpable`, `set_pdeathsig`, `waitid`, `Pid` | `kill`, `killpg` |
| `weaver-admin` | `socket`, `socketpair`, `bind`, `listen`, `connect`, `accept4` | `fcntl` | `sendmsg`, `recvmsg`, `ControlMessage`, `cmsg_space` | `getuid`, `geteuid`, `getgid`, `Uid`, `User`, `Group`, `getgrouplist` | - | **not reached** | - |
| `weaver-state` | `getsockopt` | `fcntl`, `umask` | **not reached** | **tests only**, `getuid` under `cfg(test)` | `poll` | - | - |


Four of the twenty-three named features are not reached by the shipped binary, and two
of the four are reached by tests alone. The harness names `user`, reads its peer's
credentials through `getsockopt`, which `socket` gates, and asks for its own uid only in
an integration test. State names `user` and asks for its uid only under `cfg(test)`, in
`main.rs` and in the comparison module that `postgres` compiles for tests. Admin names
`process` and launches nothing through `nix`, its agents starting under
`std::process::Command` and `systemd-run`, so no fork, wait or prctl call needs it.
State names `uio` and moves no descriptor over its socket, so none of the message calls
that need it are made. Each is a feature the manifest names and no line of the shipped
crate needs, and each is **[3]**.

## The compiled closure of each crate alone at default features, thirty-six crates

The basis in one sentence: every crate `cargo tree -e normal,no-proc-macro` lists for a
package at its default features, by name and version, counted once however many groups
pull it. Grouped by what pulls them, so the operator can see what one election costs,
and a package's compiled count in the table above is the sum of the groups it takes,
`cfg-if` and `equivalent` being the two pulled by more than one group.

- **The serialization family, six**, in every crate but the pure member: `serde`,
  `serde_core`, `serde_json`, `itoa`, `zmij`, `memchr`. `weaver-traits` takes the
  first two alone, having no JSON to write.
- **The socket layer, five**, in the four socket crates: `nix`, `libc`, `bitflags`,
  `cfg-if`, `memoffset`, the last brought by `nix`'s `socket` feature.
- **The hash, seven**, in admin and analysis: `sha2`, `digest`, `block-buffer`,
  `crypto-common`, `generic-array`, `typenum`, `cpufeatures`, and `cfg-if` again under
  `cpufeatures`.
- **The YAML parser, six**, in types under `config` and so in admin: `serde_yaml_ng`,
  `unsafe-libyaml`, `indexmap`, `hashbrown` 0.17, `equivalent`, `ryu`.
  `unsafe-libyaml` is the C library's logic carried into Rust under `unsafe`, which is
  what **[1]** weighs.
- **SQLite, eight**, in state under `sqlite`, which is its default: `rusqlite`,
  `libsqlite3-sys`, `hashlink`, `hashbrown` 0.15, `foldhash` 0.1, `fallible-iterator`
  0.3, `fallible-streaming-iterator`, `smallvec`. The `bundled` feature compiles the
  SQLite amalgamation into the binary, which is **[6]**.
- **The tensor reader, four**, in analysis: `safetensors`, `hashbrown` 0.16, `foldhash`
  0.2, `allocator-api2`, and `equivalent` again under `hashbrown`.

So `weaver-traits` is 2, the three JSON crates are 6 each, the harness and the gate
are 6 + 5 = 11, admin is 6 + 5 + 7 + 6 = 24, state is 6 + 5 + 8 = 19, and analysis is
6 + 7 + 4 + 2 = 19, the 2 being `cfg-if` and `equivalent`.

Three versions of `hashbrown` and two of `foldhash` stand in one lock, which is the
three between thirty-six and thirty-three. Each version has its own puller, `hashlink`,
`safetensors` and `indexmap` for the three `hashbrown` and two of those `hashbrown` for
the two `foldhash`, and no one crate compiles more than one of each at default features,
so this is lock hygiene rather than shipped surface, noted as **[8]**.

## The compiled closure of each crate alone at the features the deploy names

`deploy/update-stack.sh` builds the workspace with `weaver-harness/pyworker`,
`weaver-state/sqlite` and `weaver-state/postgres`, and installs `worker`,
`pyworker`, `weaver-admin`, `weaver-gate`, `weaver-spu` and `weaver-state`. Two
of those features change the picture.

**Each closure below is complete, read as `cargo tree -p <crate> -e normal,no-proc-macro
--locked --offline --features <feature>` diffed against the same command without the
feature**, by name and version, so a count re-derives from the list beside it. Two
crates a reader of the lock will look for are absent: `rustversion`, which is in the
build-dependency set above, and `portable-atomic`, which `pyo3` takes only on a target
without 64-bit atomics, which this one is not.

**`weaver-state` with `postgres`: nineteen compiled crates become sixty-nine, fifty
added, sixty-eight by name.** The `postgres` crate is the synchronous face of
`tokio-postgres`, and the fifty, grouped:

- **The async runtime the hypothesis expected away, fifteen:** `tokio` 1.53, `mio`,
  `socket2`, `tokio-util`, `pin-project-lite`, `futures-core`, `futures-channel`,
  `futures-sink`, `futures-task`, `futures-util`, `parking_lot`, `parking_lot_core`,
  `lock_api`, `scopeguard`, `log`.
- **The wire protocol, twelve:** `postgres`, `tokio-postgres`, `postgres-protocol`,
  `postgres-types`, `byteorder`, `bytes`, `base64`, `percent-encoding`, `phf`,
  `phf_shared`, `siphasher`, and `fallible-iterator` 0.2 beside the 0.3 that
  `rusqlite` holds, the one name this closure compiles at two versions.
- **The authentication stack, sixteen:** `md-5`, `hmac`, `sha2` 0.11, `digest` 0.11,
  `block-buffer` 0.12, `crypto-common` 0.2, `cpufeatures` 0.3, `hybrid-array`,
  `typenum`, `const-oid`, `chacha20`, `rand` 0.10, `rand_core`, `getrandom`, `cmov`,
  `ctutils`.
- **String preparation, six:** `stringprep`, `unicode-bidi`, `unicode-normalization`,
  `unicode-properties`, `tinyvec`, `tinyvec_macros`.
- **And `whoami`**, one.

The member speaks to a local server over a socket, and the crate that speaks for it is
a network client with a network client's dependencies. The `sha2` here is 0.11, a
second major beside admin's and analysis's 0.10, with `digest`, `block-buffer`,
`crypto-common` and `cpufeatures` at a second major beside it, so the deployed stack
carries both lines across its binaries though no one binary compiles both. This is
**[5]**.

**`weaver-harness` with `pyworker`: eleven compiled crates become fifteen, four added.**
`pyo3` 0.27, `pyo3-ffi`, `once_cell` and `unindent`, and the `pyworker` binary links
`libpython` at run time, which no count here carries. The feature is the package's, so
the four are compiled for `worker`, the Rust worker in the same crate, as well, and
`worker` references none of them and keeps none of them, per the symbol census below.
Which of the two a box runs is its `worker-binary` entry, and the deploy installs both.
This is **[2]**.

**The two engines of state, on the same basis.** `sqlite` is state's default feature,
so the nineteen-crate closure above is the default build, and the build without SQLite
is `--no-default-features`, eleven crates: the eight of the SQLite group are what the
default adds. `weaver-types` with `config` is twelve crates in place of six, the six of
the YAML parser's group, and admin is the one consumer that turns it on.

## Each shipped binary, as the deploy builds it

`deploy/update-stack.sh` builds once, `cargo build --release --locked --workspace
--features "$FEATURES"`, where `FEATURES` is
`weaver-spu/cuda,weaver-harness/pyworker,weaver-state/sqlite,weaver-state/postgres`, and
cargo resolves one set of features per dependency across every member it builds. Admin
takes `weaver-types` with `config`, so the one build compiles `weaver-types` with the
YAML parser for every member that depends on it. The compiled closure of each binary is
its package's subtree of `cargo tree --workspace --features "$FEATURES" -e
normal,no-proc-macro --locked --offline --prefix depth --no-dedupe`, one figure per
package, so `worker` and `pyworker` share the harness's. The same tree without
`weaver-spu` gives every in-scope subtree unchanged, `weaver-spu`'s one contribution to
them being `nix`'s `dir` feature.

| Binary | Package | Compiled, crate alone | Compiled, as the deploy builds it | By name |
|---|---|---|---|---|
| `worker` | `weaver-harness` | 15 | 21 | 21 |
| `pyworker` | `weaver-harness` | 15 | 21 | 21 |
| `weaver-gate` | `weaver-gate` | 11 | 17 | 17 |
| `weaver-admin` | `weaver-admin` | 24 | 24 | 24 |
| `weaver-state` | `weaver-state` | 69 | 75 | 73 |

Unification adds the same six crates to the compiled closure of `worker`, `pyworker`,
`weaver-gate` and `weaver-state`: `serde_yaml_ng`, `unsafe-libyaml`, `indexmap`,
`hashbrown` 0.17, `equivalent` and `ryu`. It adds nothing to admin, which elected them.
State's by-name figure is two short because it then compiles `hashbrown` at 0.15 and
0.17 and `fallible-iterator` at 0.2 and 0.3. `weaver-analysis` ships a binary the
deploy does not install, and its closure is nineteen either way.

**What reaches the binary.** A release build of the in-scope members at the base, with
the deploy's member features, gives these symbol counts for the crates the calls turn
on, read from each binary's demangled symbols:

| Crate | `worker` | `pyworker` | `weaver-gate` | `weaver-admin` | `weaver-state` |
|---|---|---|---|---|---|
| `serde_yaml_ng` | 0 | 0 | 0 | 419 | 0 |
| `unsafe-libyaml` | 0 | 0 | 0 | 52 | 0 |
| `indexmap` | 0 | 0 | 0 | 13 | 0 |
| `pyo3` | 0 | 569 | - | - | - |
| `tokio` | - | - | - | - | 694 |
| `tokio-postgres` | - | - | - | - | 854 |
| `rusqlite` | - | - | - | - | 153 |
| `sha2` | - | - | - | 6 | 7 |

A dash is a crate outside that binary's compiled closure. **The YAML parser is compiled
for five binaries and present in one**: no code of it survives into `worker`,
`pyworker`, the gate or state, whose code never calls the parse. `pyo3` is present in
`pyworker` alone. The unified build widens what is compiled and trusted at build time,
and it does not widen what runs.

**`nix` is one build at the union of what its crates name**: `socket`, `fs`, `uio`,
`user`, `poll`, `process` and `signal` from the four crates read here, `dir` from
`weaver-spu`, and `memoffset` and `feature` as those imply. The union adds no crate to
any closure, `memoffset` being in every socket crate already. It compiles every module
those features gate once, and each binary keeps only the calls its own code makes.

## The judgment calls, each the operator's

Each is re-read on the three readings above and against the crate rules the corpus
carries, and an alternative that neither makes effective is not offered.

1. **`serde_yaml_ng` in `weaver-types`**, behind `config`, for one purpose: parsing
   an agent's declaration, in `config::parse` and the trace-sink check it runs. It is
   the rule's own example, one purpose and a library, and it brings `unsafe-libyaml`.
   Only admin turns the feature on, and its code is present in `weaver-admin` alone.
   As the deploy builds the workspace it is compiled for every binary whose package
   depends on `weaver-types`, and none of them keeps any of it. There are two calls.
   The first is whether YAML earns `unsafe-libyaml` in the binary that runs as root,
   the alternative being a declaration format the wire already has, JSON, or a parser
   of the subset the declaration uses. The second, if it stays, is whether its being
   compiled for four more binaries matters, and the ways to stop that are a parse
   admin carries in its own crate or admin built on its own with the feature, since a
   feature on a shared crate cannot be scoped to one member of a single workspace
   build.
2. **`pyo3` in `pyworker`.** The Python loop is a documented route, the `worker-binary`
   entry chooses it per box, and the deploy installs `pyworker` beside `worker` on every
   box whether or not the entry names it. The cost is four compiled crates, compiled for
   both of the harness's binaries once the feature is on, and the interpreter at run
   time, `pyo3` present in `pyworker` and absent from `worker`. The call is whether the
   deployed stack should carry the interpreter's binary at all where the entry names the
   Rust worker, and whether `pyworker` belongs in the default deploy or behind an
   election.
3. **The `nix` feature sets.** Four named features are not reached by the shipped
   binary, per the table above: `user` in the harness and in state, `process` in
   admin and `uio` in state. A feature no call reaches is compiled and then left out
   by the linker, so none of the four adds code to a binary, and each widens what is
   compiled and trusted at build time. The two `user` reaches are by tests alone, and
   a test's need is dev-time under the rule. Removal is a workspace question: `nix`
   compiles once at the union of every member's features, so dropping a feature from
   one crate changes nothing while another member names it, `user` being named by the
   gate and admin, `process` by the harness and the gate, and `uio` by the harness and
   admin. The call is whether each crate's manifest should still name only what its
   calls need, for the reader and for the day the union narrows, and whether an
   instrument should hold it so, since nothing today refuses a feature no call needs.
4. **`sha2` in `weaver-admin`**, for two functions: a file's digest in the
   inventory and a declaration's digest, both sha256 to hex. Two functions and
   a library, and the same library analysis takes for the capture digests. Two
   binaries link it either way: `weaver-analysis` depends on no workspace crate, by
   its Spec's section 1 as its manifest records, so no shared helper can serve both,
   and a helper placed in a floor crate would make the hash compiled for every binary
   that depends on that crate while reducing neither of these two. The call is
   whether two functions in admin are the rule's case.
5. **`postgres` in the deployed `weaver-state`.** The store charter elects the
   engine and the deploy builds it, so this is a documented election and not
   a stray import, and it is also the async runtime and the network client the
   hypothesis excluded, fifty compiled crates for one engine, the runtime and the
   client both present in the binary. The options are the operator's: keep it as
   elected, stop building the feature in the default deploy so that a deployment
   elects it, or serve the store on SQLite alone until a deployment elects otherwise.
6. **`rusqlite` bundled.** The `bundled` feature compiles SQLite's amalgamation
   into the binary rather than linking the system library, which fixes the
   version the record is written under at the cost of carrying the C source's
   surface. The call is which of the two the record's custody prefers.
7. **`safetensors` in `weaver-analysis`.** It reads the residual columns'
   tensors and their dtype, which is what the lens is for, so it reads as
   required by operations. Named here so the reading is the operator's and not
   assumed.
8. **Lock hygiene.** Three `hashbrown` and two `foldhash` at default features, and
   under `postgres` two `fallible-iterator` minors in one compiled closure and two
   `sha2` majors with their four companions across the stack. As the deploy builds
   it, state's closure also compiles a second `hashbrown`, which comes through the
   unused `weaver-types` edge of call 9. The lock cannot unify the `hashbrown` three,
   each puller naming its own major, so the call is whether to carry them, to wait on
   the pullers, or to drop a puller, `safetensors` and `indexmap` being the two this
   report already weighs under **[7]** and **[1]**.
9. **The two unused internal edges**, the cheapest removals this report finds, each a
   manifest change.
   - **`weaver-state` on `weaver-types`.** No unit of state's source or tests
     references it, and `weaver-state-Spec` section 1 says so: the link is declared
     at the charter's section 1 and kept on the operator's ruling of 2026-09-14 for
     a consumer the charter's section 5 holds open. As the deploy builds the
     workspace it is the edge that compiles the YAML parser's six crates for state,
     seventy-five crates in place of sixty-nine, though none of them reaches the
     binary. The call is whether the held link is worth that build surface until
     its consumer lands, or is dropped and re-declared with the consumer.
   - **`weaver-diagnostic` on `weaver-traits`.** No unit of diagnostic's source or
     tests references it. `weaver-diagnostic-Spec` section 1 names the dependency
     "for the message model the replayed contributions carry", and the source does
     not bear that out, so either the Spec sentence or the manifest line is wrong.
     Its manifest test asserts the dependency is present by name, which holds the
     edge without anything using it. Removing it changes no external count,
     `weaver-traits` bringing only `serde` and `serde_core`, which diagnostic takes
     already. The call is which of the Spec sentence and the manifest changes, and
     that is a Spec act rather than this report's.

## How each claim was checked

Every claim of use, reach, compilation or presence in this report, with the instrument
that settles it. "Shipped source" is a crate's `src/` with every item under
`#[cfg(test)]` or `#[cfg(all(test, ...))]` removed, and every file declared only by
such a `mod`, counted by a script that strips them before matching the crate's
identifier.

| Claim | How checked | Result |
|---|---|---|
| Every direct external dependency is referenced by its crate's shipped source | the script, per crate and per dependency, and `git grep` for the zeros | 24 of 24 referenced |
| Every direct internal dependency is referenced by its crate's source | the same, and `git grep` of each identifier across the crate, tests included | 7 of 9: `weaver-diagnostic` on `weaver-traits` and `weaver-state` on `weaver-types` referenced nowhere |
| Each crate's workspace dependencies are the ones its row names | `cargo tree -e normal --depth 1`, workspace members | all ten rows match |
| `pyo3` is referenced only by `pyworker`'s target | `git grep -l pyo3` under the harness's `src/` | `src/bin/pyworker/main.rs` and `py_loop.rs` only |
| `pyo3` is compiled for both harness binaries and present in `pyworker` alone | the package's tree at the deploy's features, and the symbol census | compiled for both, 0 symbols in `worker`, 569 in `pyworker` |
| The YAML parser is present in admin alone | the census | present in `weaver-admin`, 0 symbols in `worker`, `pyworker`, the gate and state |
| The YAML parser serves one purpose | `git grep serde_yaml_ng` in `weaver-types` | two call sites, `config::parse` and the check it calls |
| `sha2` serves two functions in admin | `git grep sha2` in admin | two call sites, `inventory.rs` lines 603 and 857 |
| `postgres` brings the async runtime into state's binary | the census | `tokio` 694 and `tokio-postgres` 854 symbols |
| Each `nix` feature reading of the table | `cargo check` and `cargo check --tests` with the feature dropped, forty-six checks | as the table, two checks uninformative through `signal`'s implication |
| Admin launches nothing through `nix` | `git grep` for `fork`, `waitpid`, `prctl` in admin, and the check above | no call, the five matches being comments, its launches being `std::process::Command` and `systemd-run` |
| Each crate's compiled closure and its group sums | `cargo tree -p <crate> -e normal,no-proc-macro` per crate | all ten sum from the groups |
| Each feature's addition is complete | the tree with the feature diffed against the tree without it | `postgres` 50, `pyworker` 4, `config` 6, `sqlite` 8, each list matching |
| Each binary's compiled closure as the deploy builds it | one workspace tree at the deploy's string, split per package root, one figure per package | as the table, and unchanged without `weaver-spu` |
| The host-side set | the tree over normal and build edges less the compiled tree, per crate at the deploy's features | 22 by name and version, 13 and 9 |
| `weaver-analysis` depends on no workspace crate | `cargo tree --depth 1` and its manifest | holds, its manifest citing its Spec's section 1 |

## What this report does not do

It does not change a manifest, a Spec, a process file or a line of code, per
the epic. It does not read `weaver-spu` or `weaver-web`. It does not judge
transitive crates one by one below the groups above, since each group stands
or falls with its puller. It reads the lock at `e39f1339` and goes stale with
the next manifest change, which is why the counts are dated.
