"""Generates the declared import set, per python-spu-Spec section 8.

Each launch runs the real serving path, python -m python_spu.server and, where given,
the zipapp, with --declare-imports: the process records the modules it holds at
admission and after its first generation, then exits. The union of this run's
launches is the device's half, imports-cpu.txt or imports-cuda.txt, and it replaces
that half whole, never merged with what the half held: a module this run no longer
records leaves the half. The other half is not touched. The tiny model is built in a
separate process, so building it adds nothing to what is recorded.

Usage: python scripts/declare_imports.py --device cpu [--zipapp PATH] [--model DIR]
"""
import argparse
import datetime
import json
import os
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
from python_spu.client import LocalProcess  # noqa: E402

PACKAGE = ROOT / "src" / "python_spu"

HEADER = """\
# The modules python-spu may hold on {device} after admission and after its first
# generation, per python-spu-Spec section 8: one clean run of the real serving path,
# launched with -m and, where the stamp says so, from the zipapp. Written whole by
# scripts/declare_imports.py --device {device}, replacing what this half held, and
# reviewed, never edited by hand. The process is judged against the union of this
# half and the other device's.
#
# {stamp}
"""


def write_half(path, device, recorded, stamp):
    """Writes a device's half whole: the header and the recorded names, sorted. What
    the file held before is not read, so a name this run did not record is gone. An
    empty record is refused, never written as a half that declares nothing."""
    if not recorded:
        raise ValueError(f"the {device} run recorded no modules, so no half is written")
    Path(path).write_text(HEADER.format(device=device, stamp=stamp)
                          + "".join(f"{name}\n" for name in sorted(recorded)))

TINY = r'''
import sys, torch
from transformers import Qwen2Config, Qwen2ForCausalLM, PreTrainedTokenizerFast
from tokenizers import Tokenizer, models, pre_tokenizers
path = sys.argv[1]
vocab = {'[UNK]': 0, '<|im_start|>': 1, '<|im_end|>': 2, 'system': 3, 'user': 4,
         'assistant': 5, 'hello': 6, 'world': 7, 'be': 8, 'precise': 9}
vocab.update({f'word{i}': i for i in range(10, 64)})
backend = Tokenizer(models.WordLevel(vocab, unk_token='[UNK]'))
backend.pre_tokenizer = pre_tokenizers.WhitespaceSplit()
PreTrainedTokenizerFast(tokenizer_object=backend, unk_token='[UNK]', eos_token='<|im_end|>',
    additional_special_tokens=['<|im_start|>', '<|im_end|>']).save_pretrained(path)
torch.manual_seed(726)
Qwen2ForCausalLM(Qwen2Config(vocab_size=64, hidden_size=32, intermediate_size=64,
    num_hidden_layers=2, num_attention_heads=4, num_key_value_heads=2,
    max_position_embeddings=256, bos_token_id=1, eos_token_id=2)).save_pretrained(path)
'''


def launch(command, arguments, model, record, device):
    with tempfile.TemporaryDirectory() as scratch:
        process = LocalProcess(Path(scratch) / "stderr.txt", command=command,
                               arguments=[*arguments, "--declare-imports", str(record)])
        identity = [{"role": "system", "content": [{"type": "text", "text": "be precise"}]}]
        instruction = {"decoder": {
            "model-binding": {"artifact": str(model), "devices": [0]},
            "residual-readout-election": False, "surprisal-election": True,
            "identity": identity,
            "tunable-values": {"seed": 11, "context-capacity": 256, "max-tokens-per-turn": 4}}}
        try:
            answer = process.ask({"kind": "admit", "instruction": instruction})
            assert answer["payload"] == {"kind": "answer", "body": {"kind": "admitted"}}, answer
            decode = process.channels[1]
            decode.send({"kind": "open", "session": "declare", "messages": identity})
            assert decode.receive() == {"kind": "opened"}
            decode.send({"kind": "append_and_generate", "turn": "declare-turn", "delta": [
                {"role": "user", "content": [{"type": "text", "text": "hello world"}]}]})
        except BaseException:
            process.close()
            raise
        # The declaring process exits on its own once the first generation is
        # recorded, so the channels stay open until it has, within a bound.
        code = process.close(timeout=600, keep_open=True)
        if code != 0:
            raise RuntimeError(f"the declaring launch exited {code}: "
                               + (Path(scratch) / "stderr.txt").read_text())


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--device", choices=("cpu", "cuda"), required=True)
    parser.add_argument("--zipapp", type=Path)
    parser.add_argument("--model", type=Path)
    args = parser.parse_args(argv)
    # Each input path is read before any launch, by lstat and stat rather than by
    # exists(), which answers False for a path it may not look at, so a missing,
    # misspelled or unreadable one refuses by name instead of reaching a launch.
    try:
        if args.zipapp is not None and not stat.S_ISREG(os.stat(args.zipapp).st_mode):
            raise ValueError(f"--zipapp {args.zipapp} is not a file")
        if args.model is not None and not stat.S_ISDIR(os.stat(args.model).st_mode):
            raise ValueError(f"--model {args.model} is not a directory")
        return _declare(args)
    except (OSError, ValueError) as error:
        print(f"declare_imports: {error}", file=sys.stderr)
        return 1


def _declare(args):
    arguments = ["--cpu-experiment"] if args.device == "cpu" else []
    with tempfile.TemporaryDirectory() as scratch:
        model = args.model
        if model is None:
            model = Path(scratch) / "tiny-qwen2"
            subprocess.run([sys.executable, "-c", TINY, str(model)], check=True, timeout=600)
        record = Path(scratch) / "record.txt"
        env_path = os.environ.get("PYTHONPATH")
        os.environ["PYTHONPATH"] = os.pathsep.join(filter(None, [str(ROOT / "src"), env_path]))
        launch(None, arguments, model, record, args.device)
        if args.zipapp is not None:
            launch([sys.executable, str(args.zipapp.resolve())], arguments, model, record,
                   args.device)
        recorded = {line.strip() for line in record.read_text().splitlines() if line.strip()}
    half = PACKAGE / f"imports-{args.device}.txt"
    try:
        before = {line.strip() for line in half.read_text().splitlines()
                  if line.strip() and not line.startswith("#")}
    except FileNotFoundError:
        before = set()
    stamp = (f"generated {datetime.date.today().isoformat()}, "
             f"python {sys.version.split()[0]}, "
             f"{'with the zipapp' if args.zipapp else 'without the zipapp'}, "
             f"model {args.model.name if args.model else 'the tiny one'}")
    write_half(half, args.device, recorded, stamp)
    print(json.dumps({"device": args.device, "recorded": len(recorded),
                      "added": len(recorded - before), "dropped": len(before - recorded)}))
    return 0


if __name__ == "__main__":
    sys.exit(main())
