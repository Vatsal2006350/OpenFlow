"""Record OpenFlow migration source-size and developer-build measurements."""

import csv;
import pathlib;
import statistics;
import subprocess;
import sys;
import time;

glob ROOT = pathlib.Path(__file__).resolve().parent.parent;
glob OUTPUT = ROOT / "docs" / "jac-migration-metrics.csv";
glob BASELINE_LINES = 16163;
glob FIELDS = [
    "timestamp_utc", "kind", "label", "value", "unit", "command",
    "rust_lines", "swift_lines", "typescript_lines", "jac_lines",
    "total_source_lines", "lines_saved_vs_baseline", "notes"
];


def current_counts() -> dict[str, int] {
    counts = {"rust": 0, "swift": 0, "typescript": 0, "jac": 0};
    roots = ["crates", "src-tauri/src", "src-tauri/macos", "jac"];
    languages = {
        ".rs": "rust",
        ".swift": "swift",
        ".ts": "typescript",
        ".tsx": "typescript",
        ".jac": "jac"
    };

    for relative_root in roots {
        source_root = ROOT / relative_root;
        if not source_root.exists() {
            continue;
        }
        for path in source_root.rglob("*") {
            language = languages.get(path.suffix);
            excluded = any(
                part in {"node_modules", ".jac", "dist", "build", "packages", ".venv"}
                for part in path.parts
            );
            if path.is_file() and language is not None and not excluded {
                for line in path.read_text(encoding="utf-8", errors="replace").splitlines() {
                    if line.strip() {
                        counts[language] += 1;
                    }
                }
            }
        }
    }
    return counts;
}


def record(kind: str, label: str, value: str, unit: str, command: str, notes: str) -> None {
    counts = current_counts();
    total = sum(counts.values());
    row = {
        "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "kind": kind,
        "label": label,
        "value": value,
        "unit": unit,
        "command": command,
        "rust_lines": str(counts["rust"]),
        "swift_lines": str(counts["swift"]),
        "typescript_lines": str(counts["typescript"]),
        "jac_lines": str(counts["jac"]),
        "total_source_lines": str(total),
        "lines_saved_vs_baseline": str(BASELINE_LINES - total),
        "notes": notes
    };
    OUTPUT.parent.mkdir(parents=True, exist_ok=True);
    needs_header = not OUTPUT.exists() or OUTPUT.stat().st_size == 0;
    with OUTPUT.open("a", newline="", encoding="utf-8") as handle {
        writer = csv.DictWriter(handle, fieldnames=FIELDS);
        if needs_header {
            writer.writeheader();
        }
        writer.writerow(row);
    }
}


def run() -> None {
    args = sys.argv[1:];
    if not args or args[0] == "snapshot" {
        counts = current_counts();
        total = sum(counts.values());
        saved = BASELINE_LINES - total;
        rust_lines = counts["rust"];
        swift_lines = counts["swift"];
        ts_lines = counts["typescript"];
        jac_lines = counts["jac"];
        note = "Nonblank physical source lines; app sources only, generated assets excluded.";
        record("source_snapshot", "current", str(total), "lines", "", note);
        print(f"Source lines: {total} (Rust {rust_lines}, Swift {swift_lines}, TypeScript {ts_lines}, Jac {jac_lines})");
        print(f"Lines saved vs baseline: {saved}");
        return;
    }

    if args[0] != "benchmark" or len(args) < 4 {
        print("Usage: jac run scripts/migration_metrics.jac snapshot");
        print("   or: jac run scripts/migration_metrics.jac benchmark LABEL RUNS [--cwd DIR] -- COMMAND [ARGS...]");
        raise SystemExit(2);
    }

    label = args[1];
    runs = max(1, int(args[2]));
    command_start = 3;
    working_directory = ROOT;
    if len(args) > 4 and args[3] == "--cwd" {
        working_directory = ROOT / args[4];
        command_start = 5;
    }
    command = args[command_start:];
    if command and command[0] == "--" {
        command = command[1:];
    }
    if not command {
        print("Benchmark command is required.");
        raise SystemExit(2);
    }

    samples: list[float] = [];
    for index in range(runs) {
        started = time.perf_counter();
        result = subprocess.run(command, cwd=working_directory);
        elapsed = time.perf_counter() - started;
        if result.returncode != 0 {
            raise SystemExit(result.returncode);
        }
        samples.append(elapsed);
        record("dev_iteration", f"{label}#{index + 1}", f"{elapsed:.3f}", "seconds", " ".join(command), "One successful command run.");
    }

    median = statistics.median(samples);
    record("dev_iteration_median", label, f"{median:.3f}", "seconds", " ".join(command), f"Median of {runs} successful run(s).");
    print(f"{label}: median {median:.3f}s across {runs} run(s)");
}


with entry {
    run();
}
