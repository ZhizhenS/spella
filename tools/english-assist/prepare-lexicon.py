#!/usr/bin/env python3
"""Verify frozen inputs, build offline metadata, and compare the pinned output.

The normal build never downloads data or updates the expected manifest. Use the
explicit --refresh-manifest authoring option after reviewing a data/compiler change.
"""
from __future__ import annotations

import argparse
import gzip
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def digest(path: Path) -> str:
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def checked_file(root: Path, record: dict) -> Path:
    name = record["file"]
    if not isinstance(name, str) or Path(name).name != name or "\\" in name:
        raise ValueError("Frozen source filename must be a single basename")
    path = root / name
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"Missing or linked frozen source: {name}")
    if path.stat().st_size != record["bytes"] or digest(path) != record["sha256"]:
        raise ValueError(f"Frozen source size/hash mismatch: {name}")
    return path


def verify_source(root: Path, record: dict) -> Path:
    path = checked_file(root, record)
    total = 0
    sha = hashlib.sha256()
    opener = gzip.open if path.suffix == ".gz" else Path.open
    with opener(path, "rb") as stream:
        while block := stream.read(1024 * 1024):
            total += len(block)
            if total > record["uncompressedBytes"]:
                raise ValueError(f"Frozen source expands beyond pinned size: {path.name}")
            sha.update(block)
    if total != record["uncompressedBytes"] or sha.hexdigest() != record["uncompressedSha256"]:
        raise ValueError(f"Frozen uncompressed source mismatch: {path.name}")
    return path


def validate_output(root: Path, manifest: dict) -> Path:
    if manifest.get("schema") != 2 or manifest.get("format") != "RTENLX02":
        raise ValueError("Unexpected offline lexicon schema/format")
    if manifest.get("file") != "english-lexicon-v2.bin":
        raise ValueError("Unexpected offline lexicon filename")
    path = checked_file(root, manifest)
    with path.open("rb") as stream:
        if stream.read(8) != b"RTENLX02":
            raise ValueError("Unexpected offline lexicon magic")
    counts = manifest.get("counts", {})
    if any(type(counts.get(key)) is not int or counts[key] <= 0 for key in ("words", "pinyin", "mappings")):
        raise ValueError("Offline lexicon must include words and pinyin mappings")
    return path


def require_same_manifest(actual: dict, expected: dict) -> None:
    if actual != expected:
        raise ValueError("Offline lexicon differs from the reviewed expected manifest; review before explicitly refreshing it")


def prepare(repo: Path, output: Path, refresh: bool = False, verify_only: bool = False) -> dict:
    sources_root = repo / "data/dict/raw/english-assist"
    source_manifest = json.loads((sources_root / "SOURCES.json").read_text(encoding="utf-8-sig"))
    if source_manifest.get("schema") != 1:
        raise ValueError("Unexpected frozen sources manifest schema")
    inputs = {key: verify_source(sources_root, source_manifest["sources"][key]) for key in ("ecdict", "cedict")}
    chinese_root = repo / "data/dict/raw/wanxiang-base"
    for key in ("chinese_zi", "chinese_words"):
        inputs[key] = verify_source(chinese_root, source_manifest["chineseFrequency"][key])
    for name, record in source_manifest["licenses"].items():
        checked_file(sources_root, {"file": name, **record})
    frequency = repo / "data/dict/raw/english/frequency.txt"
    expected_path = repo / "data/dict/english-lexicon.json"
    expected = None if refresh else json.loads(expected_path.read_text(encoding="utf-8-sig"))
    if expected:
        for key, path in {**inputs, "frequency": frequency}.items():
            records = {**source_manifest["sources"], **source_manifest["chineseFrequency"]}
            content_sha = records[key]["uncompressedSha256"] if key in inputs else digest(path)
            if expected["sources"][key] != {"file": path.name, "sha256": digest(path), "content_sha256": content_sha}:
                raise ValueError(f"Lexicon input differs from reviewed output sources: {key}")
    if verify_only:
        if refresh:
            raise ValueError("Cannot refresh a manifest in verify-only mode")
        validate_output(output, expected)
        packaged = json.loads((output / "english-lexicon-v2.json").read_text(encoding="utf-8-sig"))
        require_same_manifest(packaged, expected)
        return expected
    output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".english-lexicon-", dir=output) as task_temp:
        stage = Path(task_temp)
        actual_path = stage / "english-lexicon-v2.json"
        subprocess.run([
            sys.executable, str(repo / "tools/english-assist/build-lexicon.py"),
            "--ecdict", str(inputs["ecdict"]), "--cedict", str(inputs["cedict"]),
            "--frequency", str(frequency),
            "--chinese-zi", str(inputs["chinese_zi"]), "--chinese-words", str(inputs["chinese_words"]),
            "--output", str(stage / "english-lexicon-v2.bin"),
            "--manifest", str(actual_path),
        ], check=True, stdout=subprocess.PIPE)
        actual = json.loads(actual_path.read_text(encoding="utf-8-sig"))
        image = validate_output(stage, actual)
        if expected is not None:
            require_same_manifest(actual, expected)
        if refresh:
            expected_path.write_text(json.dumps(actual, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
        shutil.copyfile(image, output / image.name)
        shutil.copyfile(actual_path, output / actual_path.name)
    return actual


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--refresh-manifest", action="store_true")
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[2]
    try:
        result = prepare(repo, args.output.resolve(), args.refresh_manifest, args.verify_only)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(f"Offline lexicon preparation failed: {error}") from error
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
