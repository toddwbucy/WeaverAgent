"""Where the Rust oracle's binary is, asked of cargo rather than assumed (#42).

Cargo writes the oracle under CARGO_TARGET_DIR where the environment sets one, and a
relative value resolves against the directory cargo runs in, so the one authority on
the location is cargo's own metadata: `target_directory` for the oracle's manifest,
read with the environment the suite runs in."""
import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MANIFEST = ROOT / 'oracle' / 'Cargo.toml'


def executable() -> Path:
    metadata = subprocess.run(
        ['cargo', 'metadata', '--format-version', '1', '--no-deps', '--locked',
         '--manifest-path', str(MANIFEST)],
        cwd=ROOT, check=True, capture_output=True, text=True)
    return Path(json.loads(metadata.stdout)['target_directory']) / 'debug' / 'python-spu-oracle'
