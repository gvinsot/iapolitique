#!/usr/bin/env python3
"""Calibrate the screening grid (prompts/screening.yaml) against the d3-mini decision service.

Standard library only. Three sub-commands:

  run    send every labelled sample with its section's questions to /v1/systemone
         (at most --workers concurrent requests) and store the raw answers;
  eval   replay the decision rule of AGENTS.md §5.3 on stored answers with the grid's
         thresholds (optionally overridden with --set) and print accuracy, false rejects,
         false accepts and gray-zone rate;
  sweep  per criterion, print separation metrics (AUC) and candidate thresholds.

Examples (from the repository root):

  python docs/calibration/bench_screening.py run
  python docs/calibration/bench_screening.py eval --set idea.realism.reject_below=0.15
  python docs/calibration/bench_screening.py sweep --section idea

Samples (JSON Lines): {id, section: idea|contribution|report, text, expected: accept|reject,
fails: [criterion keys the text must fail], gray_ok, split: tune|holdout, tags, ...}; contributions add `parent`
and `kind`, reports add `parent` and `reason_label`. A criterion not listed in `fails` is
considered passed when the sample is expected to be accepted, and unknown otherwise.
"""

from __future__ import annotations

import argparse
import concurrent.futures
import hashlib
import json
import pathlib
import re
import statistics
import sys
import time
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[2]
DEFAULT_GRID = ROOT / "prompts" / "screening.yaml"
DEFAULT_SAMPLES = ROOT / "docs" / "calibration" / "screening_samples.jsonl"
DEFAULT_RESULTS = ROOT / "docs" / "calibration" / "screening_results.jsonl"
DEFAULT_URL = "http://192.168.1.27:8001/v1/systemone"
SECTIONS = ("idea", "contribution", "report")
KIND_LABELS = {"amelioration": "amélioration", "objection": "objection"}


# --------------------------------------------------------------------------------------
# Minimal YAML reader for the subset used by prompts/screening.yaml: block mappings, block
# sequences, plain or quoted scalars, comments. No anchors, flow collections or block
# scalars (the loader raises on anything it does not understand).
# --------------------------------------------------------------------------------------


class YamlError(ValueError):
    pass


def _strip_comment(line: str) -> str:
    out, quote, i = [], None, 0
    while i < len(line):
        c = line[i]
        if quote:
            out.append(c)
            if c == "\\" and quote == '"' and i + 1 < len(line):
                out.append(line[i + 1])
                i += 1
            elif c == quote:
                if quote == "'" and i + 1 < len(line) and line[i + 1] == "'":
                    out.append("'")
                    i += 1
                else:
                    quote = None
        elif c in "\"'" and (not out or out[-1] in " :-[{,"):
            quote = c
            out.append(c)
        elif c == "#" and (not out or out[-1] in " \t"):
            break
        else:
            out.append(c)
        i += 1
    return "".join(out).rstrip()


def _scalar(tok: str):
    tok = tok.strip()
    if not tok:
        return None
    if tok[0] == '"':
        if not tok.endswith('"') or len(tok) < 2:
            raise YamlError(f"unterminated string: {tok}")
        return json.loads(tok)
    if tok[0] == "'":
        if not tok.endswith("'") or len(tok) < 2:
            raise YamlError(f"unterminated string: {tok}")
        return tok[1:-1].replace("''", "'")
    if tok[0] in "[{|>&*!":
        raise YamlError(f"unsupported YAML construct: {tok}")
    low = tok.lower()
    if low in ("true", "false"):
        return low == "true"
    if low in ("null", "~"):
        return None
    if re.fullmatch(r"[-+]?\d+", tok):
        return int(tok)
    if re.fullmatch(r"[-+]?(\d+\.\d*|\.\d+)([eE][-+]?\d+)?", tok):
        return float(tok)
    return tok


def _split_key(text: str):
    """Split `key: value` honouring a quoted key. Returns (key, rest) or None."""
    if text[:1] in "\"'":
        q = text[0]
        end = text.find(q, 1)
        while q == "'" and end != -1 and text[end + 1 : end + 2] == "'":
            end = text.find(q, end + 2)
        if end == -1 or not text[end + 1 :].startswith(":"):
            return None
        return _scalar(text[: end + 1]), text[end + 2 :]
    m = re.match(r"^([^\s:\"'][^:]*?):(\s|$)", text)
    if not m:
        return None
    return m.group(1), text[m.end() :]


def load_yaml(text: str):
    lines = []
    for n, raw in enumerate(text.splitlines(), 1):
        if "\t" in raw[: len(raw) - len(raw.lstrip())]:
            raise YamlError(f"line {n}: tab indentation")
        s = _strip_comment(raw)
        if s.strip():
            lines.append((len(s) - len(s.lstrip(" ")), s.strip(), n))
    value, pos = _parse_block(lines, 0, lines[0][0] if lines else 0)
    if pos != len(lines):
        raise YamlError(f"line {lines[pos][2]}: unexpected indentation")
    return value


def _parse_block(lines, pos, indent):
    if pos >= len(lines):
        return None, pos
    if lines[pos][1].startswith("- ") or lines[pos][1] == "-":
        return _parse_seq(lines, pos, indent)
    return _parse_map(lines, pos, indent)


def _parse_seq(lines, pos, indent):
    out = []
    while pos < len(lines) and lines[pos][0] == indent and (lines[pos][1] == "-" or lines[pos][1].startswith("- ")):
        _, text, n = lines[pos]
        rest = text[1:].lstrip()
        if not rest:
            value, pos = _parse_block(lines, pos + 1, lines[pos + 1][0]) if pos + 1 < len(lines) and lines[pos + 1][0] > indent else (None, pos + 1)
        elif _split_key(rest):
            # mapping item: first key on the dash line, following keys indented like it
            child_indent = indent + (len(text) - len(rest))
            lines[pos] = (child_indent, rest, n)
            value, pos = _parse_map(lines, pos, child_indent)
        else:
            value, pos = _scalar(rest), pos + 1
        out.append(value)
    return out, pos


def _parse_map(lines, pos, indent):
    out = {}
    while pos < len(lines) and lines[pos][0] == indent:
        _, text, n = lines[pos]
        kv = _split_key(text)
        if kv is None:
            raise YamlError(f"line {n}: expected `key: value`, got {text!r}")
        key, rest = kv
        if key in out:
            raise YamlError(f"line {n}: duplicate key {key!r}")
        if rest.strip():
            out[key], pos = _scalar(rest), pos + 1
        elif pos + 1 < len(lines) and lines[pos + 1][0] > indent:
            out[key], pos = _parse_block(lines, pos + 1, lines[pos + 1][0])
        elif pos + 1 < len(lines) and lines[pos + 1][0] == indent and lines[pos + 1][1].startswith("- "):
            out[key], pos = _parse_seq(lines, pos + 1, indent)  # YAML allows `key:` then `- item` at same indent
        else:
            out[key], pos = None, pos + 1
    if pos < len(lines) and lines[pos][0] > indent:
        raise YamlError(f"line {lines[pos][2]}: unexpected indentation")
    return out, pos


# --------------------------------------------------------------------------------------
# Grid
# --------------------------------------------------------------------------------------


def load_grid(path: pathlib.Path) -> dict:
    grid = load_yaml(path.read_text(encoding="utf-8"))
    if not isinstance(grid, dict) or not isinstance(grid.get("version"), str):
        raise SystemExit(f"{path}: missing string `version`")
    for section in SECTIONS:
        crits = grid.get(section)
        if not isinstance(crits, list) or not crits:
            raise SystemExit(f"{path}: section `{section}` must be a non-empty list")
        seen = set()
        for c in crits:
            where = f"{path}: {section}.{c.get('key')}"
            missing = {"key", "type", "instructions", "pass_when", "reject_below", "accept_above", "reason_code", "reason_message"} - set(c)
            if missing:
                raise SystemExit(f"{where}: missing {sorted(missing)}")
            if c["key"] in seen:
                raise SystemExit(f"{where}: duplicate key")
            seen.add(c["key"])
            if c["type"] == "noul":
                if not isinstance(c["pass_when"], bool):
                    raise SystemExit(f"{where}: pass_when must be true/false for noul")
                if "criteria" in c and set(c["criteria"]) - {"true", "false"}:
                    raise SystemExit(f"{where}: noul criteria keys must be \"true\"/\"false\"")
            elif c["type"] == "score":
                levels = c.get("criteria")
                if not isinstance(levels, list) or not 2 <= len(levels) <= 10:
                    raise SystemExit(f"{where}: score needs 2 to 10 levels")
                if not isinstance(c["pass_when"], int) or not 0 < c["pass_when"] < len(levels):
                    raise SystemExit(f"{where}: pass_when must be a level index in 1..{len(levels) - 1}")
            else:
                raise SystemExit(f"{where}: unsupported type {c['type']!r}")
            if not 0.0 <= c["reject_below"] <= c["accept_above"] <= 1.0:
                raise SystemExit(f"{where}: need 0 <= reject_below <= accept_above <= 1")
    return grid


def questions(grid: dict, section: str) -> dict:
    qs = {}
    for c in grid[section]:
        q = {"type": c["type"], "instructions": c["instructions"]}
        if c.get("criteria") is not None:
            q["criteria"] = c["criteria"]
        qs[c["key"]] = q
    return qs


def questions_hash(grid: dict, section: str) -> str:
    blob = json.dumps(questions(grid, section), ensure_ascii=False, sort_keys=True)
    return hashlib.sha256(blob.encode()).hexdigest()[:12]


def build_state(sample: dict):
    if sample["section"] == "idea":
        return sample["text"]
    if sample["section"] == "contribution":
        return {"idee": sample["parent"], "type": KIND_LABELS[sample["kind"]], "contribution": sample["text"]}
    return {"idee": sample["parent"], "motif": sample["reason_label"], "precision": sample["text"]}


def p_pass(crit: dict, answer: dict) -> float:
    if crit["type"] == "noul":
        p = float(answer["noul"])
        return p if crit["pass_when"] else 1.0 - p
    probs = {int(k): float(v) for k, v in answer["probabilities"].items()}
    return sum(v for k, v in probs.items() if k >= crit["pass_when"])


def decide(crits: list, passes: dict) -> tuple[str, str | None]:
    """Decision rule of AGENTS.md §5.3: (accepted|rejected|gray, failing criterion)."""
    for c in crits:
        if passes[c["key"]] < c["reject_below"]:
            return "rejected", c["key"]
    if all(passes[c["key"]] >= c["accept_above"] for c in crits):
        return "accepted", None
    return "gray", None


# --------------------------------------------------------------------------------------
# run
# --------------------------------------------------------------------------------------


def call(url: str, state, qs: dict, timeout: float, retries: int = 3) -> tuple[dict, float, int]:
    body = json.dumps({"state": state, "questions": qs}, ensure_ascii=False).encode()
    for attempt in range(retries):
        req = urllib.request.Request(url, body, {"content-type": "application/json"})
        start = time.perf_counter()
        try:
            with urllib.request.urlopen(req, timeout=timeout) as r:
                data = json.load(r)
            return data["answers"], (time.perf_counter() - start) * 1000, data.get("usage", {}).get("input_tokens", 0)
        except urllib.error.HTTPError as e:
            if e.code < 500 or attempt == retries - 1:
                raise RuntimeError(f"HTTP {e.code}: {e.read()[:300]!r}") from e
        except (urllib.error.URLError, TimeoutError):
            if attempt == retries - 1:
                raise
        time.sleep(2**attempt)
    raise AssertionError("unreachable")


def cmd_run(args) -> None:
    grid = load_grid(args.grid)
    samples = [s for s in read_jsonl(args.samples) if selected(args, s)]
    hashes = {s: questions_hash(grid, s) for s in SECTIONS}
    print(f"grid {grid['version']} hashes {hashes}; {len(samples)} samples; {args.workers} workers", flush=True)

    def work(sample):
        answers, ms, tokens = call(args.url, build_state(sample), questions(grid, sample["section"]), args.timeout)
        return {"id": sample["id"], "section": sample["section"], "questions_hash": hashes[sample["section"]],
                "latency_ms": round(ms, 1), "input_tokens": tokens, "answers": answers}

    results, errors = {}, 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.workers) as pool:
        futures = {pool.submit(work, s): s for s in samples}
        for i, fut in enumerate(concurrent.futures.as_completed(futures), 1):
            try:
                r = fut.result()
                results[r["id"]] = r
            except Exception as e:  # keep going, report at the end
                errors += 1
                print(f"error on {futures[fut]['id']}: {e}", file=sys.stderr)
            if i % 25 == 0:
                print(f"  {i}/{len(samples)}", flush=True)

    # merge with previous results of other sections so a partial run keeps the rest
    previous = {r["id"]: r for r in read_jsonl(args.results)} if args.results.exists() else {}
    previous.update(results)
    order = [s["id"] for s in read_jsonl(args.samples)]
    with open(args.results, "w", encoding="utf-8", newline="\n") as f:
        for sid in order:
            if sid in previous:
                f.write(json.dumps(previous[sid], ensure_ascii=False) + "\n")
    lat = [r["latency_ms"] for r in results.values()]
    tok = [r["input_tokens"] for r in results.values()]
    if lat:
        print(f"done: {len(results)} ok, {errors} errors, latency p50={statistics.median(lat):.0f} ms "
              f"max={max(lat):.0f} ms, input tokens p50={statistics.median(tok):.0f} max={max(tok)}")
    if errors:
        sys.exit(1)


# --------------------------------------------------------------------------------------
# eval / sweep
# --------------------------------------------------------------------------------------


def read_jsonl(path: pathlib.Path) -> list:
    with open(path, encoding="utf-8") as f:
        return [json.loads(line) for line in f if line.strip()]


def apply_overrides(grid: dict, overrides: list) -> None:
    for item in overrides:
        m = re.fullmatch(r"(\w+)\.(\w+)\.(reject_below|accept_above)=([0-9.]+)", item)
        if not m:
            raise SystemExit(f"bad --set {item!r}, expected section.key.reject_below|accept_above=value")
        section, key, field, value = m.groups()
        crit = next((c for c in grid.get(section, []) if c["key"] == key), None)
        if crit is None:
            raise SystemExit(f"unknown criterion {section}.{key}")
        crit[field] = float(value)


def joined(args, grid):
    samples = {s["id"]: s for s in read_jsonl(args.samples)}
    rows = []
    stale = 0
    for r in read_jsonl(args.results):
        s = samples.get(r["id"])
        if s is None or not selected(args, s):
            continue
        if r["questions_hash"] != questions_hash(grid, s["section"]):
            stale += 1
            continue
        crits = grid[s["section"]]
        passes = {c["key"]: p_pass(c, r["answers"][c["key"]]) for c in crits}
        rows.append((s, passes))
    if stale:
        print(f"warning: {stale} results were produced with other question wording and are ignored "
              f"(re-run `run`)", file=sys.stderr)
    return rows


def selected(args, sample: dict) -> bool:
    if args.section and sample["section"] != args.section:
        return False
    return args.split == "all" or sample.get("split", "tune") == args.split


def truth(sample: dict, key: str):
    """True if the sample should pass criterion `key`, False if it should fail, None if unknown."""
    if key in sample["fails"]:
        return False
    if sample["expected"] == "accept":
        return True
    return None


def auc(pos: list, neg: list) -> float:
    """Probability that a passing sample scores higher than a failing one (ties count half)."""
    if not pos or not neg:
        return float("nan")
    wins = sum((p > n) + 0.5 * (p == n) for p in pos for n in neg)
    return wins / (len(pos) * len(neg))


def cmd_eval(args) -> None:
    grid = load_grid(args.grid)
    apply_overrides(grid, args.set or [])
    rows = joined(args, grid)
    print(f"grid {grid['version']} ({args.grid.name}), split {args.split}, {len(rows)} scored samples\n")
    sections = [args.section] if args.section else SECTIONS
    summary = {}
    for section in sections:
        srows = [(s, p) for s, p in rows if s["section"] == section]
        if not srows:
            continue
        crits = grid[section]
        print(f"== {section} ({len(srows)} samples) ==")
        print("thresholds: " + ", ".join(f"{c['key']} <{c['reject_below']:.2f}|>={c['accept_above']:.2f}" for c in crits))
        counts = {("accept", o): 0 for o in ("accepted", "gray", "rejected")}
        counts.update({("reject", o): 0 for o in ("accepted", "gray", "rejected")})
        wrong_reason, issues = 0, []
        for s, passes in srows:
            outcome, failed = decide(crits, passes)
            counts[(s["expected"], outcome)] += 1
            if s["expected"] == "reject" and outcome == "rejected" and failed not in s["fails"]:
                wrong_reason += 1
            bad = (s["expected"] == "accept" and outcome == "rejected") or (s["expected"] == "reject" and outcome == "accepted")
            if bad or (outcome == "gray" and not s.get("gray_ok") and args.verbose) or args.verbose > 1:
                flag = "FALSE-REJECT" if s["expected"] == "accept" and outcome == "rejected" else (
                    "FALSE-ACCEPT" if s["expected"] == "reject" and outcome == "accepted" else outcome)
                detail = " ".join(f"{k}={v:.2f}" for k, v in passes.items())
                issues.append(f"  {flag:12s} {s['id']} [{failed or '-'}] {detail}\n      {s['text'][:110]}")
        n_acc = sum(v for (e, _), v in counts.items() if e == "accept")
        n_rej = sum(v for (e, _), v in counts.items() if e == "reject")
        correct = counts[("accept", "accepted")] + counts[("reject", "rejected")]
        gray = counts[("accept", "gray")] + counts[("reject", "gray")]
        decided = len(srows) - gray
        # with a perfect second opinion the gray zone costs a call but is resolved correctly
        final_ok = correct + gray
        print(f"expected accept ({n_acc}): accepted {counts[('accept', 'accepted')]}, gray {counts[('accept', 'gray')]}, "
              f"REJECTED {counts[('accept', 'rejected')]}")
        print(f"expected reject ({n_rej}): rejected {counts[('reject', 'rejected')]}, gray {counts[('reject', 'gray')]}, "
              f"ACCEPTED {counts[('reject', 'accepted')]}")
        print(f"false reject rate {counts[('accept', 'rejected')] / max(n_acc, 1):.1%} | "
              f"false accept rate {counts[('reject', 'accepted')] / max(n_rej, 1):.1%} | "
              f"gray-zone rate {gray / len(srows):.1%} | accuracy on decided {correct / max(decided, 1):.1%} | "
              f"accuracy with ideal second opinion {final_ok / len(srows):.1%} | wrong reason {wrong_reason}")
        for line in issues:
            print(line)
        print()
        summary[section] = dict(counts={f"{e}->{o}": v for (e, o), v in counts.items()}, gray=gray, n=len(srows))
    if args.json:
        print(json.dumps(summary, indent=1))


def cmd_sweep(args) -> None:
    grid = load_grid(args.grid)
    rows = joined(args, grid)
    grid_points = [0.02, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 0.95]
    for section in [args.section] if args.section else SECTIONS:
        srows = [(s, p) for s, p in rows if s["section"] == section]
        if not srows:
            continue
        print(f"== {section} ==")
        for c in grid[section]:
            pos = sorted(p[c["key"]] for s, p in srows if truth(s, c["key"]) is True)
            neg = sorted(p[c["key"]] for s, p in srows if truth(s, c["key"]) is False)
            print(f"-- {c['key']}: {len(pos)} should pass, {len(neg)} should fail, AUC {auc(pos, neg):.3f}")
            if pos:
                print(f"   pass p_pass: min {pos[0]:.3f} p10 {pos[len(pos) // 10]:.3f} median {statistics.median(pos):.3f}")
            if neg:
                print(f"   fail p_pass: median {statistics.median(neg):.3f} p90 {neg[(9 * len(neg)) // 10]:.3f} max {neg[-1]:.3f}")
            print("   t      pass<t (false rej)  fail<t (caught)  pass>=t (auto ok)  fail>=t (leak)")
            for t in grid_points:
                fr = sum(p < t for p in pos)
                ca = sum(p < t for p in neg)
                print(f"   {t:<5.2f}  {fr:4d} ({fr / max(len(pos), 1):5.1%})     {ca:4d} ({ca / max(len(neg), 1):5.1%})"
                      f"   {len(pos) - fr:4d} ({(len(pos) - fr) / max(len(pos), 1):5.1%})    {len(neg) - ca:4d} "
                      f"({(len(neg) - ca) / max(len(neg), 1):5.1%})")
        print()


def main(argv=None) -> None:
    common = argparse.ArgumentParser(add_help=False)
    common.add_argument("--grid", type=pathlib.Path, default=DEFAULT_GRID)
    common.add_argument("--samples", type=pathlib.Path, default=DEFAULT_SAMPLES)
    common.add_argument("--results", type=pathlib.Path, default=DEFAULT_RESULTS)
    common.add_argument("--section", choices=SECTIONS)
    common.add_argument("--split", choices=("tune", "holdout", "all"), default="all",
                        help="tune: samples used to choose wording and thresholds; holdout: written afterwards")
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    run = sub.add_parser("run", parents=[common], help="query the decision service and store raw answers")
    run.add_argument("--url", default=DEFAULT_URL)
    run.add_argument("--workers", type=int, default=4, help="concurrent requests (the service has 4 slots)")
    run.add_argument("--timeout", type=float, default=30.0)
    ev = sub.add_parser("eval", parents=[common], help="apply the decision rule with the grid thresholds")
    ev.add_argument("--set", action="append", metavar="SECTION.KEY.FIELD=VALUE")
    ev.add_argument("-v", "--verbose", action="count", default=0, help="-v: list unexpected gray cases, -vv: every sample")
    ev.add_argument("--json", action="store_true")
    sub.add_parser("sweep", parents=[common], help="per-criterion threshold candidates")
    args = ap.parse_args(argv)
    if args.cmd == "run":
        args.workers = max(1, min(args.workers, 4))
        cmd_run(args)
    elif args.cmd == "eval":
        cmd_eval(args)
    else:
        cmd_sweep(args)


if __name__ == "__main__":
    main()
