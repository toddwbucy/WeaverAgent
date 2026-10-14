"""The suite finds the oracle where cargo built it, per #42: under CARGO_TARGET_DIR where
the environment sets one, and under the oracle crate's own target otherwise.

Perturbation: hard-code oracle/target/debug in `oracle_location.executable`, as the
suite did before #42, and the first test fails."""
from pathlib import Path

import oracle_location

ROOT = Path(__file__).resolve().parents[1]


def test_the_oracle_is_found_under_cargo_target_dir(tmp_path, monkeypatch):
    monkeypatch.setenv('CARGO_TARGET_DIR', str(tmp_path))
    assert oracle_location.executable() == tmp_path / 'debug' / 'python-spu-oracle'


def test_the_oracle_is_found_under_its_own_target_without_one(monkeypatch):
    monkeypatch.delenv('CARGO_TARGET_DIR', raising=False)
    assert oracle_location.executable() == ROOT / 'oracle' / 'target' / 'debug' / 'python-spu-oracle'
