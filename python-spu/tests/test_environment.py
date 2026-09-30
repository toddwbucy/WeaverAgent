"""The environment's rules, per python-spu-Spec sections 2, 5 and 8."""
import ast
import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

import pytest

from python_spu.client import LocalProcess
from python_spu.transport import ChannelFault, Closed

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
import build_zipapp  # noqa: E402
import declare_imports  # noqa: E402
import installed_set  # noqa: E402
import tree_digest  # noqa: E402
from python_spu import import_set, loaded_code  # noqa: E402
from python_spu.client import reap  # noqa: E402


def rust_spu():
    """The Rust SPU this box serves, from admin's configuration."""
    stated = os.environ.get("WEAVER_RUST_SPU")
    if stated:
        return stated
    try:
        return Path("/etc/weaver/admin/spu-binary").read_text().strip()
    except OSError:
        return None


def test_expf_resolves_to_the_libm_the_rust_spu_links():
    """Section 5's bit-for-bit claim rests on the port's expf being the one the Rust
    SPU's f32::exp calls. The port loads libm through ctypes, and this reads the
    library the process mapped against the one the Rust binary links."""
    spu = rust_spu()
    if not spu or not os.path.exists(spu):
        pytest.skip("no Rust SPU on this box to compare against")
    from python_spu import candle_chain
    assert candle_chain.expf(0.0) == 1.0
    mapped = {os.path.realpath(line.split()[-1]) for line in open("/proc/self/maps")
              if re.search(r"/libm[.-][^/]*$", line.strip())}
    linked = [line for line in subprocess.run(["ldd", spu], capture_output=True, text=True,
                                              check=True, timeout=60).stdout.splitlines()
              if line.strip().startswith("libm.so")]
    assert len(linked) == 1, linked
    assert mapped == {os.path.realpath(linked[0].split("=>")[1].split()[0])}, (mapped, linked)


def serve_once(tmp_path, tiny_model, extra_environment=None):
    """A real server process through admission and one generation, with the import
    set judged as main() judges it. Answers the process's exit and stderr where it
    died or failed an exchange, and None where it answered the generation. The
    process is closed on every path, its channels first and then a bounded wait, so
    a failed exchange with the process still serving cannot hang the test."""
    environment = dict(os.environ, **(extra_environment or {}))
    saved = dict(os.environ)
    os.environ.clear()
    os.environ.update(environment)
    try:
        process = LocalProcess(tmp_path / "stderr.txt")
    finally:
        os.environ.clear()
        os.environ.update(saved)
    identity = [{"role": "system", "content": [{"type": "text", "text": "be precise"}]}]
    instruction = {"decoder": {
        "model-binding": {"artifact": str(tiny_model), "devices": [0]},
        "residual-readout-election": False, "surprisal-election": True, "identity": identity,
        "tunable-values": {"seed": 11, "context-capacity": 256, "max-tokens-per-turn": 4}}}
    generated = False
    try:
        answer = process.ask({"kind": "admit", "instruction": instruction})
        assert answer["payload"] == {"kind": "answer", "body": {"kind": "admitted"}}
        decode = process.channels[1]
        decode.send({"kind": "open", "session": "import-set", "messages": identity})
        assert decode.receive() == {"kind": "opened"}
        decode.send({"kind": "append_and_generate", "turn": "import-set-turn", "delta": [
            {"role": "user", "content": [{"type": "text", "text": "hello world"}]}]})
        while not generated:
            generated = decode.receive()["kind"] == "generated"
    except (Closed, ChannelFault, ConnectionError, AssertionError, OSError, ValueError):
        pass
    finally:
        code = process.close()
    if generated:
        return None
    return code, (tmp_path / "stderr.txt").read_text()


def test_the_import_set_holds_through_admission_and_generation(tmp_path, tiny_model):
    """The declared set covers a clean serving run on the CPU. Where this fails the
    CPU half is stale, and scripts/declare_imports.py regenerates it for review."""
    assert serve_once(tmp_path, tiny_model) is None, (tmp_path / "stderr.txt").read_text()


def test_an_extra_import_faults_the_process(tmp_path, tiny_model):
    """The perturbation section 8 names: a module the list does not declare, here
    imported at startup through sitecustomize, faults the process at admission,
    naming the module, and it never answers."""
    site = tmp_path / "site"
    site.mkdir()
    (site / "sitecustomize.py").write_text("import xml.dom.minidom\n")
    path = os.pathsep.join([str(site), os.environ.get("PYTHONPATH", "")])
    died = serve_once(tmp_path, tiny_model, {"PYTHONPATH": path})
    assert died is not None, "the undeclared module was served past"
    code, stderr = died
    assert code == 3, (code, stderr)
    fault = json.loads(stderr.strip().splitlines()[-1])
    assert fault["import_set_violation"] == "admission"
    assert "xml.dom.minidom" in fault["undeclared"]


def test_the_zipapp_is_reproducible_and_launches(tmp_path):
    first = build_zipapp.build(ROOT / "src", tmp_path / "a.pyz", sys.executable)
    second = build_zipapp.build(ROOT / "src", tmp_path / "b.pyz", sys.executable)
    assert first.read_bytes() == second.read_bytes()
    assert first.read_bytes().startswith(b"#!" + sys.executable.encode() + b"\n")
    # The entry runs and reads the worker's vector: python-spu has no help flag, as the
    # Rust SPU has none, so an unknown parameter is refused by name before anything
    # else, in the Rust SPU's own form.
    shown = subprocess.run([str(first), "--help"], capture_output=True, text=True, timeout=120)
    assert (shown.returncode, shown.stdout) == (1, ""), shown.stderr
    assert json.loads(shown.stderr) == {"refusal": "bad_parameter",
                                        "detail": "unknown parameter --help"}
    import zipfile
    names = zipfile.ZipFile(first).namelist()
    assert {"python_spu/imports-cpu.txt", "python_spu/imports-cuda.txt"} <= set(names)
    assert "python_spu/classifier.py" not in names


def test_the_tree_digest_reads_files_and_links_and_refuses_the_rest(tmp_path):
    tree = tmp_path / "tree"
    (tree / "lib").mkdir(parents=True)
    (tree / "lib" / "a.py").write_text("a\n")
    outside = tmp_path / "outside.txt"
    outside.write_text("outside\n")
    (tree / "bin").mkdir()
    os.symlink(outside, tree / "bin" / "link")
    first = tree_digest.digest(tree)
    assert tree_digest.digest(tree) == first
    outside.write_text("changed\n")
    assert tree_digest.digest(tree) == first, "a link is read by its text and never followed"
    (tree / "lib" / "a.py").write_text("b\n")
    assert tree_digest.digest(tree) != first
    os.mkfifo(tree / "lib" / "pipe")
    with pytest.raises(ValueError):
        tree_digest.digest(tree)


def test_a_regenerated_half_replaces_itself_whole(tmp_path):
    """A device's regeneration writes its half whole, so a module the new run no
    longer records is no longer declared, unless the other half still records it.
    Perturbation: merge the recorded names into what the half held, as the first form
    did, and the dropped module stays allowed."""
    cpu, cuda = tmp_path / "imports-cpu.txt", tmp_path / "imports-cuda.txt"
    (cpu).write_text("# a header\nkept\ndropped\nshared\n")
    declare_imports.write_half(cuda, "cuda", {"shared", "cuda_only"}, "test")
    declare_imports.write_half(cpu, "cpu", {"kept", "shared"}, "test")
    declared = import_set.declared([cpu.read_text(), cuda.read_text()])
    assert declared == {"kept", "shared", "cuda_only"}, declared
    assert "dropped" not in declared
    assert cpu.read_text().startswith("# The modules python-spu may hold on cpu")


def test_the_declared_set_is_the_union_of_the_committed_halves():
    """What the process enforces is both halves and nothing else."""
    package = ROOT / "src" / "python_spu"
    halves = [(package / half).read_text() for half in import_set.HALVES]
    assert import_set.DECLARED == import_set.parse(halves[0]) | import_set.parse(halves[1])
    assert import_set.parse(halves[0]) and import_set.parse(halves[1])


def test_reap_kills_a_child_that_outlives_its_bound():
    """Perturbation: wait without a bound, and this test hangs."""
    pid = os.posix_spawn("/bin/sleep", ["/bin/sleep", "60"], dict(os.environ))
    begin = time.monotonic()
    assert reap(pid, timeout=0.5) == -9
    assert time.monotonic() - begin < 5


def test_a_failed_exchange_with_the_process_serving_does_not_hang(tmp_path):
    """An admission refused leaves the process serving, and serve_once's assertion
    fails while it lives. It closes the channels and waits within a bound. Perturbation:
    wait unbounded with the channels open, as the first form did, and this test hangs."""
    begin = time.monotonic()
    died = serve_once(tmp_path, tmp_path / "no-such-model")
    assert died is not None, "a missing model was admitted"
    assert time.monotonic() - begin < 60


def refused(command, **kwargs):
    """Runs a script and answers whether it exited non-zero having printed nothing."""
    done = subprocess.run(command, capture_output=True, text=True, timeout=120, **kwargs)
    return done.returncode != 0 and not done.stdout.strip(), done


def unreadable(path):
    """Makes a directory this process may not read, and answers it, where the process
    is not root, which reads every directory."""
    if os.geteuid() == 0:
        pytest.skip("root reads every directory")
    path.mkdir(parents=True)
    (path / "inside").write_text("x\n")
    os.chmod(path, 0o000)
    return path


def test_the_tree_digest_is_never_of_nothing(tmp_path):
    """Each root that is not a readable tree exits non-zero and prints no digest.
    Perturbations: drop the lstat check, and the link to a directory is digested; drop
    the walk's onerror, and the tree with an unreadable subdirectory is digested
    without it; drop the empty check, and the empty directory is digested."""
    (tmp_path / "file").write_text("x\n")
    (tmp_path / "empty").mkdir()
    (tmp_path / "target").mkdir()
    (tmp_path / "target" / "a").write_text("a\n")
    os.symlink(tmp_path / "target", tmp_path / "link")
    (tmp_path / "partial").mkdir()
    (tmp_path / "partial" / "a").write_text("a\n")
    roots = {"absent": tmp_path / "absent", "a file": tmp_path / "file",
             "empty": tmp_path / "empty", "a link to a directory": tmp_path / "link"}
    roots["0o000"] = unreadable(tmp_path / "walled")
    unreadable(tmp_path / "partial" / "walled")
    roots["an unreadable subdirectory"] = tmp_path / "partial"
    try:
        for label, root in roots.items():
            ok, done = refused([sys.executable, str(ROOT / "scripts" / "tree_digest.py"),
                                str(root)])
            assert ok, (label, done.returncode, done.stdout, done.stderr)
    finally:
        os.chmod(tmp_path / "walled", 0o700)
        os.chmod(tmp_path / "partial" / "walled", 0o700)


def test_the_zipapp_is_never_built_from_nothing(tmp_path):
    """A source without the package, a package lacking a file the process cannot
    start without, and a package with an unreadable subdirectory are each refused.
    Among the packages lacking a file is one without loaded_code.py, which enforce
    imports only once admission is judged, so a build without it would start and then
    fail at admission. Perturbations: drop the REQUIRED check, and the package without
    server.py builds; drop loaded_code.py from REQUIRED, and the package without it
    builds; drop the walk's onerror, and the unreadable subdirectory is skipped."""
    with pytest.raises(OSError):
        build_zipapp.build(tmp_path / "absent", tmp_path / "a.pyz")
    for absent in ("server.py", "loaded_code.py"):
        lacking = tmp_path / f"lacking-{absent}" / "python_spu"
        lacking.mkdir(parents=True)
        for name in build_zipapp.REQUIRED:
            if name != absent:
                (lacking / name).write_text("\n")
        with pytest.raises(ValueError, match=re.escape(absent)):
            build_zipapp.build(lacking.parent, lacking.parent / "lacking.pyz")
        assert not (lacking.parent / "lacking.pyz").exists()
    whole = tmp_path / "whole" / "python_spu"
    whole.mkdir(parents=True)
    for name in build_zipapp.REQUIRED:
        (whole / name).write_text("\n")
    unreadable(whole / "walled")
    try:
        with pytest.raises(PermissionError):
            build_zipapp.build(tmp_path / "whole", tmp_path / "c.pyz")
    finally:
        os.chmod(whole / "walled", 0o700)
    assert not any((tmp_path / name).exists() for name in ("a.pyz", "c.pyz"))


def test_the_zipapp_requires_both_import_set_halves():
    assert set(import_set.HALVES) <= set(build_zipapp.REQUIRED)


def served_modules(package):
    """The package's modules the serving entry point imports, transitively from
    server.py, as file names: every import in each module, a function's own among
    them, whether relative or by the package's name, read from the source rather than
    from what a run happened to import."""
    def targets(tree):
        for node in ast.walk(tree):
            if isinstance(node, ast.ImportFrom):
                assert node.level <= 1, node.lineno
                if node.level == 1:
                    base = node.module
                elif (node.module or "").split(".")[0] == "python_spu":
                    base = node.module.removeprefix("python_spu").removeprefix(".") or None
                else:
                    continue
                if base is None:
                    for alias in node.names:
                        yield alias.name if (package / f"{alias.name}.py").is_file() else None
                else:
                    yield base.split(".")[0]
            elif isinstance(node, ast.Import):
                for alias in node.names:
                    if alias.name.startswith("python_spu."):
                        yield alias.name.split(".")[1]
                    elif alias.name == "python_spu":
                        yield None
    seen, todo = {"__init__.py"}, ["server"]
    while todo:
        name = todo.pop()
        if f"{name}.py" in seen:
            continue
        seen.add(f"{name}.py")
        for target in targets(ast.parse((package / f"{name}.py").read_text())):
            if target is not None:
                todo.append(target)
    return seen


def test_the_zipapp_requires_every_module_the_server_imports():
    """Every module the serving entry point reaches is a file the build requires, so a
    source lacking one is refused at the build rather than at the admission that first
    imports it. The derivation reaches the imports made inside functions: loaded_code,
    which enforce imports only at admission, is among them. Perturbation: drop
    loaded_code.py, or any other module the server reaches, from REQUIRED, and this
    fails."""
    served = served_modules(ROOT / "src" / "python_spu")
    assert {"loaded_code.py", "import_set.py", "candle_chain.py", "family.py"} <= served
    assert served <= set(build_zipapp.REQUIRED), sorted(served - set(build_zipapp.REQUIRED))


def test_declare_imports_refuses_its_inputs_before_any_launch(tmp_path):
    """A missing model or a zipapp that is not a file refuses by name, and an empty
    record writes no half. Perturbations: drop the stat checks, and the missing model
    reaches a launch, which raises; drop the empty check, and a half declaring nothing
    is written."""
    assert declare_imports.main(["--device", "cpu", "--model", str(tmp_path / "absent")]) == 1
    assert declare_imports.main(["--device", "cpu", "--zipapp", str(tmp_path)]) == 1
    with pytest.raises(ValueError):
        declare_imports.write_half(tmp_path / "imports-cpu.txt", "cpu", set(), "test")
    assert not (tmp_path / "imports-cpu.txt").exists()


def test_smoke_refuses_a_missing_model_before_writing(tmp_path):
    """Perturbation: drop the check, and the report directory is made before the
    refused admission."""
    ok, done = refused([sys.executable, str(ROOT / "scripts" / "smoke.py"),
                        str(tmp_path / "absent"), "--output", str(tmp_path / "out" / "r.json")])
    assert ok, (done.returncode, done.stdout, done.stderr)
    assert not (tmp_path / "out").exists()


def test_an_empty_directory_changes_the_digest(tmp_path):
    """Every directory is a record, so two trees differing only by an empty directory
    digest differently. Perturbation: list files and links alone, as the first form
    did, and the two digests are equal."""
    tree = tmp_path / "tree"
    tree.mkdir()
    (tree / "a").write_text("a\n")
    before = tree_digest.digest(tree)
    (tree / "empty").mkdir()
    assert tree_digest.digest(tree) != before
    assert "dir  empty" in tree_digest.listing(tree)


SYSTEM_PYTHON = "/usr/bin/python3"


def test_the_zipapp_is_the_same_bytes_under_any_interpreter(tmp_path):
    """Stored entries carry no compressor's output, so the pinned interpreter and the
    system's build one file. Perturbation: deflate the entries, and the stored check
    fails, and where the two interpreters link different zlib builds so do the bytes."""
    if not os.path.exists(SYSTEM_PYTHON) or (
            os.path.realpath(SYSTEM_PYTHON) == os.path.realpath(sys.executable)):
        pytest.skip("no second interpreter to build with")
    pinned = build_zipapp.build(ROOT / "src", tmp_path / "pinned.pyz")
    subprocess.run([SYSTEM_PYTHON, str(ROOT / "scripts" / "build_zipapp.py"),
                    "--output", str(tmp_path / "system.pyz")], check=True, timeout=120)
    assert pinned.read_bytes() == (tmp_path / "system.pyz").read_bytes()
    import zipfile
    assert {i.compress_type for i in zipfile.ZipFile(pinned).infolist()} == {zipfile.ZIP_STORED}


def test_code_outside_the_environment_is_foreign():
    """The maps rule, on a listing: code from the prefix and the admitted system
    objects passes, the NVIDIA management library the card run mapped among them, and
    code from a temporary directory, a sibling directory sharing the prefix's name, an
    unlisted system object, a deleted file or a memfd is foreign. A data mapping is not
    judged. Perturbations: judge data mappings too, and the safetensors mapping is
    foreign; drop the deleted clause, and the deleted object under the prefix passes;
    drop the separator from the root check, and the sibling passes; drop libnvidia-*
    from the admitted names, and the management library is foreign."""
    roots = ("/opt/weaver/python-spu",)
    listing = "\n".join([
        "7f00-7f01 r-xp 00000000 00:00 1 /opt/weaver/python-spu/lib/libpython3.14.so",
        "7f01-7f02 r-xp 00000000 00:00 1 /usr/lib/libc.so.6",
        "7f02-7f03 r-xp 00000000 00:00 1 /usr/lib/libcuda.so.615.71.09",
        "7f03-7f04 r-xp 00000000 00:00 1 /usr/lib/libstdc++.so.6.0.36",
        "7f03-7f04 r-xp 00000000 00:00 1 /usr/lib/libnvidia-ml.so.615.71.09",
        "7f03-7f04 r-xp 00000000 00:00 1 /opt/weaver/python-spu-evil/x.so",
        "7f04-7f05 r--s 00000000 00:00 1 /opt/weaver/models/m/model.safetensors",
        "7f05-7f06 r--p 00000000 00:00 1 /tmp/data.bin",
        "7f06-7f07 r-xp 00000000 00:00 1 /tmp/abc/cuda_utils.cpython-314-x86_64-linux-gnu.so",
        "7f07-7f08 r-xp 00000000 00:00 1 /usr/lib/libfoo.so.1",
        "7f08-7f09 r-xp 00000000 00:00 1 /opt/weaver/python-spu/lib/x.so (deleted)",
        "7f09-7f0a r-xp 00000000 00:00 1 /memfd:jit (deleted)",
        "7f0a-7f0b r-xp 00000000 00:00 0 [vdso]",
        "7f0b-7f0c rwxp 00000000 00:00 0 ",
    ])
    assert loaded_code.foreign(listing, roots) == sorted([
        "/tmp/abc/cuda_utils.cpython-314-x86_64-linux-gnu.so",
        "/usr/lib/libfoo.so.1",
        "/opt/weaver/python-spu-evil/x.so",
        "/opt/weaver/python-spu/lib/x.so (deleted)",
        "/memfd:jit (deleted)",
    ])


def test_a_clean_process_maps_no_foreign_code():
    """The test process itself, in the locked environment, maps nothing foreign."""
    assert loaded_code.foreign_now() == []


def test_code_loaded_from_outside_faults_the_process(tmp_path, tiny_model):
    """The perturbation section 8 names for the maps rule: a shared object copied out
    of the environment and loaded at startup, as a compiler's output would be, faults
    the process at admission, naming it, and it never answers. Perturbation: drop the
    code check from enforce, and the process admits."""
    source = Path(loaded_code.prefixes()[-1]).joinpath(
        "lib", "python3.14", "site-packages", "markupsafe")
    built = next(source.glob("_speedups*.so"))
    site = tmp_path / "site"
    site.mkdir()
    # Loaded as an extension module rather than through ctypes, whose import in
    # this interpreter opens a descriptor before the package counts what it
    # inherited, which would fault the process for another reason first.
    (site / "sitecustomize.py").write_text(
        "import importlib.util\n"
        f"copy = {str(tmp_path / 'throwaway.so')!r}\n"
        f"with open({str(built)!r}, 'rb') as source, open(copy, 'wb') as target:\n"
        "    target.write(source.read())\n"
        "spec = importlib.util.spec_from_file_location('_speedups', copy)\n"
        "spec.loader.exec_module(importlib.util.module_from_spec(spec))\n")
    path = os.pathsep.join([str(site), os.environ.get("PYTHONPATH", "")])
    died = serve_once(tmp_path, tiny_model, {"PYTHONPATH": path})
    assert died is not None, "code loaded from outside the environment was served past"
    code, stderr = died
    assert code == 3, (code, stderr)
    fault = json.loads(stderr.strip().splitlines()[-1])
    assert fault["loaded_code_violation"] == "admission"
    assert str(tmp_path / "throwaway.so") in fault["foreign"]


def test_the_environment_carries_no_triton_and_torch_compiles_nothing():
    """The lock leaves triton out, the environment holds none, and the package turns
    torch's native DSL registration off before torch is imported."""
    import importlib.util
    import python_spu
    assert importlib.util.find_spec("triton") is None
    for lock in ("requirements.lock", "requirements-test.lock"):
        assert not re.search(r"^triton==", (ROOT / lock).read_text(), re.M), lock
    assert python_spu and os.environ["TORCH_DISABLE_NATIVE_JIT"] == "1"


def test_the_locked_environment_holds_the_test_lock_and_nothing_more():
    """The guard the install runs after pip, run as the install runs it, isolated, on
    the environment the suite runs in: every distribution is the test lock's pin, at
    its version, beside the interpreter's own pip."""
    done = subprocess.run([sys.executable, "-I", "-B",
                           str(ROOT / "scripts" / "installed_set.py"),
                           str(ROOT / "requirements-test.lock")],
                          capture_output=True, text=True, timeout=120)
    assert done.returncode == 0, done.stderr


def distribution(root, name, version):
    info = root / f"{name}-{version}.dist-info"
    info.mkdir(parents=True)
    (info / "METADATA").write_text(f"Metadata-Version: 2.1\nName: {name}\n"
                                   f"Version: {version}\n")


def test_the_installed_set_refuses_what_the_lock_does_not_pin(tmp_path):
    """pip install adds what a lock lists and removes nothing, so a prefix installed
    from an earlier lock keeps what the new one dropped, as triton was kept. A
    distribution the lock does not pin, one it pins that is absent, one at another
    version and one found twice are each refused by name, and so is a lock line that
    is not a pin. Perturbations: drop the not-in-the-lock branch, the missing clause,
    the version comparison or the twice clause, or skip a line that is not a pin, and
    this fails."""
    locked = installed_set.pins((ROOT / "requirements-test.lock").read_text())
    assert installed_set.differences(locked) == []
    distribution(tmp_path / "extra", "triton", "3.8.0")
    assert installed_set.differences(locked, [*sys.path, str(tmp_path / "extra")]) == [
        "not in the lock: triton==3.8.0"]
    assert installed_set.differences({**locked, "absent": "1.0"}) == [
        "missing: absent==1.0"]
    assert installed_set.differences({**locked, "blake3": "0.0.1"}) == [
        f"another version: blake3=={locked['blake3']}, the lock pins 0.0.1"]
    distribution(tmp_path / "twice", "blake3", locked["blake3"])
    assert installed_set.differences(locked, [*sys.path, str(tmp_path / "twice")]) == [
        f"found twice: blake3 {locked['blake3']}, {locked['blake3']}"]
    with pytest.raises(ValueError, match="line 2"):
        installed_set.pins("blake3==1.0 \\\n-e ./somewhere\n")
    with pytest.raises(ValueError, match="pins nothing"):
        installed_set.pins("# a comment\n")
    lock = tmp_path / "short.lock"
    lock.write_text((ROOT / "requirements-test.lock").read_text() + "absent==1.0\n")
    done = subprocess.run([sys.executable, "-I", "-B",
                           str(ROOT / "scripts" / "installed_set.py"), str(lock)],
                          capture_output=True, text=True, timeout=120)
    assert (done.returncode, done.stderr) == (1, "installed_set: missing: absent==1.0\n")
    done = subprocess.run([sys.executable, "-I", "-B",
                           str(ROOT / "scripts" / "installed_set.py"),
                           str(tmp_path / "no.lock")],
                          capture_output=True, text=True, timeout=120)
    assert done.returncode == 2, done.stderr


def children_of(pid):
    """The processes whose parent is pid, read from every /proc/<n>/stat, which a
    non-dumpable parent does not hide."""
    kids = []
    for entry in os.listdir("/proc"):
        if not entry.isdigit():
            continue
        try:
            stat = open(f"/proc/{entry}/stat").read()
            cmdline = open(f"/proc/{entry}/cmdline").read().replace("\0", " ").strip()
        except OSError:
            continue
        if int(stat.rsplit(")", 1)[1].split()[1]) == pid:
            kids.append((int(entry), cmdline))
    return kids


def test_the_spu_is_one_process_after_admission_and_after_generation(tmp_path, tiny_model, instruction):
    """python-spu-Spec section 8: the served process starts no child, so what the import
    set and the maps rule judge is all it runs. Perturbation: leave transformers'
    progress bars on, and the multiprocessing resource tracker is a child from the load
    on."""
    from python_spu.wire import dump
    process = LocalProcess(tmp_path / "stderr.txt", arguments=["--cpu-experiment"])
    try:
        body = dump(instruction)
        body["decoder"]["model-binding"]["artifact"] = str(tiny_model)
        admitted = process.ask({"kind": "admit", "instruction": body})
        assert admitted["payload"] == {"kind": "answer", "body": {"kind": "admitted"}}
        assert children_of(process.pid) == []
        decode = process.channels[1]
        decode.send({"kind": "open", "session": "s",
                     "messages": [{"role": "system", "content": [{"type": "text", "text": "be precise"}]}]})
        assert decode.receive() == {"kind": "opened"}
        decode.send({"kind": "append_and_generate", "turn": "t", "delta": [
            {"role": "user", "content": [{"type": "text", "text": "hello world"}]}]})
        while decode.receive()["kind"] != "generated":
            pass
        assert children_of(process.pid) == []
    finally:
        assert process.close() == 0, (tmp_path / "stderr.txt").read_text()


SPAWN_WATCH = r'''
import sys, traceback
spawned = []
def hook(event, args):
    if event in ("os.posix_spawn", "os.spawn", "os.fork", "os.forkpty", "os.exec",
                 "subprocess.Popen", "os.system"):
        spawned.append(f"{event} {str(args)[:120]}")
sys.addaudithook(hook)
import multiprocessing.util as mu
original = mu.spawnv_passfds
def spawnv_passfds(path, args, passfds):
    spawned.append(f"multiprocessing spawnv_passfds {args}")
    return original(path, args, passfds)
mu.spawnv_passfds = spawnv_passfds
import python_spu.server
from python_spu.engine import HFEngine
import torch
from python_spu.session import Session
from python_spu.wire import SpuInstruction
engine = HFEngine(sys.argv[1], [0], cpu=True)
instruction = SpuInstruction.model_validate({"decoder": {
    "model-binding": {"artifact": "fixture", "devices": [0]},
    "residual-readout-election": False, "surprisal-election": True, "identity": [],
    "tunable-values": {"seed": 11, "context-capacity": 256, "max-tokens-per-turn": 4}}})
session = Session(engine, instruction.decoder, 256, 4, 11)
session.open([{"role": "system", "content": [{"type": "text", "text": "be precise"}]}])
session.generate("t", [{"role": "user", "content": [{"type": "text", "text": "hello world"}]}],
                 lambda frame: None)
engine.close()
print("\n".join(spawned))
'''


def test_nothing_is_spawned_even_for_a_moment(tiny_model):
    """A child that exits at once, like ldconfig run by ctypes.util.find_library, is never
    seen as a child, so every spawn is watched in a fresh interpreter through the import,
    an admission and a generation, the sampler's libm load among them. Perturbations: leave find_library as the standard
    library has it, and cuda.pathfinder's import runs ldconfig. Leave the progress bars
    on, and the resource tracker is spawned."""
    done = subprocess.run([sys.executable, "-c", SPAWN_WATCH, str(tiny_model)],
                          capture_output=True, text=True, timeout=300,
                          env=dict(os.environ, PYTHONPATH=str(ROOT / "src"), CUDA_VISIBLE_DEVICES=""))
    assert done.returncode == 0, done.stderr
    assert done.stdout.strip() == "", done.stdout
