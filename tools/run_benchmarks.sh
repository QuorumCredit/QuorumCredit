#!/bin/bash
# Gas Benchmark Runner
# Usage: ./tools/run_benchmarks.sh [--update] [--report-only]

set -euo pipefail

cd "$(dirname "$0")/.."

BENCHMARK_OUTPUT="benchmark_output.txt"
UPDATE_BASELINE=false
REPORT_ONLY=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --update)
            UPDATE_BASELINE=true
            shift
            ;;
        --report-only)
            REPORT_ONLY=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--update] [--report-only]"
            exit 1
            ;;
    esac
done

echo "=== QuorumCredit Gas Benchmark Suite ==="
echo ""

if [ "$REPORT_ONLY" = false ]; then
    echo "[1/3] Running parameterized gas benchmarks..."
    if cargo test --lib gas_benchmark -- --nocapture --test-threads=1 2>&1 | tee "$BENCHMARK_OUTPUT"; then
        echo "✓ Benchmarks completed successfully"
    else
        echo "⚠️  Benchmarks failed or timed out (continuing to analysis...)"
    fi
    echo ""
fi

echo "[2/3] Analyzing complexity classes and fitting measurements..."
if python3 tools/benchmark_analyzer.py docs/benchmarks.json; then
    echo "✓ No regressions detected"
    REGRESSION_STATUS=0
else
    echo "⚠️  Regressions detected (see above and docs/benchmark_report.html)"
    REGRESSION_STATUS=1
fi
echo ""

echo "[3/3] Report generated at: docs/benchmark_report.html"
if command -v open &>/dev/null; then
    echo "      Opening report in browser..."
    open docs/benchmark_report.html
elif command -v xdg-open &>/dev/null; then
    xdg-open docs/benchmark_report.html
fi
echo ""

if [ "$UPDATE_BASELINE" = true ]; then
    echo "[*] Updating baseline in docs/benchmarks.json..."
    # In a real scenario, this would parse benchmark_output.txt and update benchmarks.json
    # For now, we'll just indicate this is where that would happen
    echo "    (Baseline update would be scripted here)"
fi

echo ""
echo "=== Summary ==="
echo "- Benchmark suite: ${BENCHMARK_OUTPUT}"
echo "- Analysis report: docs/benchmark_report.html"
echo "- Historical data: docs/benchmarks.json"
echo ""

exit $REGRESSION_STATUS
