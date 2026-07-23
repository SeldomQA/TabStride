#!/usr/bin/env python3
"""Compare one-process-per-command CLI latency with `tabstride client`.

`wait-ms` isolates process/transport fixed cost and needs no browser.
`status` includes the diagnostic command's browser-wait policy.
`todomvc-snapshot` uses a caller-supplied live TodoMVC session.
`five-step-flow` submits five daemon-local waits in one Flow RPC.
"""

from __future__ import annotations

import argparse
import json
import math
import subprocess
import time
from pathlib import Path


def percentile(values: list[float], percentile_value: float) -> float:
    ordered = sorted(values)
    index = max(0, math.ceil(percentile_value * len(ordered)) - 1)
    return ordered[index]


def summary(values: list[float]) -> dict[str, float | int]:
    return {
        "samples": len(values),
        "p50_ms": round(percentile(values, 0.50), 3),
        "p95_ms": round(percentile(values, 0.95), 3),
        "p99_ms": round(percentile(values, 0.99), 3),
        "min_ms": round(min(values), 3),
        "max_ms": round(max(values), 3),
    }


def run_cli(
    binary: Path,
    scenario: str,
    session: str | None,
    flow_file: Path,
    samples: int,
    warmups: int,
) -> list[float]:
    durations: list[float] = []
    for index in range(samples + warmups):
        started = time.perf_counter_ns()
        command = [str(binary), "--json", "wait-ms", "1ms"]
        if scenario == "status":
            command = [str(binary), "--json", "status"]
        if scenario == "todomvc-snapshot":
            assert session is not None
            command = [str(binary), "--json", "snapshot", "--session", session]
        if scenario == "five-step-flow":
            command = [
                str(binary), "--json", "flow", "run", str(flow_file),
                "--session", "benchmark",
            ]
        subprocess.run(
            command,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
        if index >= warmups:
            durations.append(elapsed_ms)
    return durations


def run_native(
    binary: Path, scenario: str, session: str | None, samples: int, warmups: int
) -> list[float]:
    process = subprocess.Popen(
        [str(binary), "client"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=1,
    )
    assert process.stdin is not None
    assert process.stdout is not None
    durations: list[float] = []
    try:
        for index in range(samples + warmups):
            request = {
                "id": f"bench-{index}",
                "method": "tool.wait_ms",
                "params": {"duration_ms": 1},
            }
            if scenario == "status":
                request = {
                    "id": f"bench-{index}",
                    "method": "system.status",
                    "params": {},
                }
            if scenario == "todomvc-snapshot":
                assert session is not None
                request = {
                    "id": f"bench-{index}",
                    "method": "tool.snapshot",
                    "params": {"session_id": session},
                }
            if scenario == "five-step-flow":
                request = {
                    "id": f"bench-{index}",
                    "method": "flow.run",
                    "params": {
                        "session_id": "benchmark",
                        "flow": {
                            "name": "benchmark-five-step",
                            "timeout": "1s",
                            "steps": [
                                {"wait_ms": {"duration_ms": 1}},
                                {"wait_ms": {"duration_ms": 1}},
                                {"wait_ms": {"duration_ms": 1}},
                                {"wait_ms": {"duration_ms": 1}},
                                {"wait_ms": {"duration_ms": 1}},
                            ],
                        },
                        "variables": {},
                    },
                }
            started = time.perf_counter_ns()
            process.stdin.write(json.dumps(request, separators=(",", ":")) + "\n")
            process.stdin.flush()
            response = json.loads(process.stdout.readline())
            if "error" in response:
                raise RuntimeError(response["error"])
            elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
            if index >= warmups:
                durations.append(elapsed_ms)
    finally:
        process.stdin.close()
        process.wait(timeout=5)
    return durations


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/release/tabstride"))
    parser.add_argument(
        "--scenario",
        choices=["wait-ms", "status", "todomvc-snapshot", "five-step-flow"],
        default="wait-ms",
    )
    parser.add_argument("--session")
    parser.add_argument(
        "--flow-file",
        type=Path,
        default=Path("examples/flows/benchmark-five-step.yaml"),
    )
    parser.add_argument("--samples", type=int, default=100)
    parser.add_argument("--warmups", type=int, default=5)
    parser.add_argument("--out", type=Path)
    args = parser.parse_args()
    if args.samples < 1 or args.warmups < 0:
        parser.error("--samples must be positive and --warmups non-negative")
    if args.scenario == "todomvc-snapshot" and not args.session:
        parser.error("--session is required for --scenario todomvc-snapshot")

    result = {
        "scenario": args.scenario,
        "session_id": args.session,
        "cli": summary(
            run_cli(
                args.binary,
                args.scenario,
                args.session,
                args.flow_file,
                args.samples,
                args.warmups,
            )
        ),
        "native_client": summary(
            run_native(args.binary, args.scenario, args.session, args.samples, args.warmups)
        ),
    }
    output = json.dumps(result, indent=2, ensure_ascii=False)
    print(output)
    if args.out:
        args.out.parent.mkdir(parents=True, exist_ok=True)
        args.out.write_text(output + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
