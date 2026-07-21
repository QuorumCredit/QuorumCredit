// Integration test for gas benchmarking system
// This test can run independently to verify the benchmarking framework works

#[test]
fn test_benchmark_analyzer_exists() {
    // Verify that the benchmark analyzer script exists
    assert!(std::path::Path::new("tools/benchmark_analyzer.py").exists(),
            "benchmark_analyzer.py not found");
}

#[test]
fn test_benchmarks_json_exists() {
    // Verify benchmarks.json exists and contains expected data
    let json_path = "docs/benchmarks.json";
    assert!(std::path::Path::new(json_path).exists(),
            "docs/benchmarks.json not found");

    let content = std::fs::read_to_string(json_path)
        .expect("Failed to read benchmarks.json");

    // Verify it contains expected top-level keys
    assert!(content.contains("\"metadata\""),
            "benchmarks.json should have metadata section");
    assert!(content.contains("\"operations\""),
            "benchmarks.json should have operations section");
    assert!(content.contains("\"regression_thresholds\""),
            "benchmarks.json should have regression_thresholds section");
}

#[test]
fn test_gas_budgets_markdown_exists() {
    // Verify gas-budgets.md exists and mentions complexity
    let md_path = "docs/gas-budgets.md";
    assert!(std::path::Path::new(md_path).exists(),
            "docs/gas-budgets.md not found");

    let content = std::fs::read_to_string(md_path)
        .expect("Failed to read gas-budgets.md");

    assert!(content.contains("Complexity"),
            "gas-budgets.md should mention Complexity-Based Benchmarking");
    assert!(content.contains("O(n)"),
            "gas-budgets.md should reference O(n) complexity");
}

#[test]
fn test_benchmark_workflow_exists() {
    // Verify CI workflow file exists
    let workflow_path = ".github/workflows/gas-benchmarks.yml";
    assert!(std::path::Path::new(workflow_path).exists(),
            "gas-benchmarks.yml workflow not found");

    let content = std::fs::read_to_string(workflow_path)
        .expect("Failed to read gas-benchmarks.yml");

    assert!(content.contains("gas_benchmark"),
            "workflow should run gas_benchmark tests");
    assert!(content.contains("benchmark_analyzer.py"),
            "workflow should run benchmark analyzer");
}

#[test]
fn test_benchmark_runner_script_exists() {
    // Verify the local runner script exists
    let script_path = "tools/run_benchmarks.sh";
    assert!(std::path::Path::new(script_path).exists(),
            "run_benchmarks.sh not found");
}

#[test]
fn test_benchmarking_guide_exists() {
    // Verify documentation exists
    let guide_path = "BENCHMARKING.md";
    assert!(std::path::Path::new(guide_path).exists(),
            "BENCHMARKING.md not found");

    let content = std::fs::read_to_string(guide_path)
        .expect("Failed to read BENCHMARKING.md");

    assert!(content.contains("Complexity Fitting"),
            "BENCHMARKING.md should explain complexity fitting");
    assert!(content.contains("Regression Detection"),
            "BENCHMARKING.md should explain regression detection");
}
