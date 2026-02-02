use criterion::{black_box, criterion_group, criterion_main, Criterion};
use rustant_tools::registry::{Tool, ToolRegistry};
use serde_json::json;
use std::sync::Arc;

fn bench_tool_registry(c: &mut Criterion) {
    c.bench_function("registry_register_builtin_tools", |b| {
        b.iter(|| {
            let mut registry = ToolRegistry::new();
            rustant_tools::register_builtin_tools(&mut registry, std::env::temp_dir());
            registry
        })
    });

    c.bench_function("registry_get_tool", |b| {
        let mut registry = ToolRegistry::new();
        rustant_tools::register_builtin_tools(&mut registry, std::env::temp_dir());
        b.iter(|| registry.get(black_box("calculator")))
    });

    c.bench_function("registry_list_definitions", |b| {
        let mut registry = ToolRegistry::new();
        rustant_tools::register_builtin_tools(&mut registry, std::env::temp_dir());
        b.iter(|| registry.list_definitions())
    });

    c.bench_function("registry_list_names", |b| {
        let mut registry = ToolRegistry::new();
        rustant_tools::register_builtin_tools(&mut registry, std::env::temp_dir());
        b.iter(|| registry.list_names())
    });
}

fn bench_calculator_tool(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut registry = ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, std::env::temp_dir());
    let calc = registry.get("calculator").unwrap();

    c.bench_function("calculator_simple_add", |b| {
        b.iter(|| {
            rt.block_on(async {
                calc.execute(black_box(json!({"expression": "2 + 3"})))
                    .await
            })
        })
    });

    c.bench_function("calculator_complex_expression", |b| {
        b.iter(|| {
            rt.block_on(async {
                calc.execute(black_box(json!({"expression": "(100 + 200) * 3 / 4 - 50"})))
                    .await
            })
        })
    });

    c.bench_function("calculator_nested_parens", |b| {
        b.iter(|| {
            rt.block_on(async {
                calc.execute(black_box(
                    json!({"expression": "((1 + 2) * (3 + 4)) / ((5 - 6) + 10)"}),
                ))
                .await
            })
        })
    });
}

fn bench_echo_tool(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut registry = ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, std::env::temp_dir());
    let echo = registry.get("echo").unwrap();

    c.bench_function("echo_short_message", |b| {
        b.iter(|| {
            rt.block_on(async {
                echo.execute(black_box(json!({"message": "hello world"})))
                    .await
            })
        })
    });

    let long_msg = "x".repeat(10_000);
    c.bench_function("echo_long_message", |b| {
        let args = json!({"message": long_msg});
        b.iter(|| rt.block_on(async { echo.execute(black_box(args.clone())).await }))
    });
}

fn bench_datetime_tool(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut registry = ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, std::env::temp_dir());
    let dt = registry.get("datetime").unwrap();

    c.bench_function("datetime_now", |b| {
        b.iter(|| rt.block_on(async { dt.execute(black_box(json!({}))).await }))
    });

    c.bench_function("datetime_with_format", |b| {
        b.iter(|| {
            rt.block_on(async {
                dt.execute(black_box(json!({"format": "%Y-%m-%d %H:%M:%S"})))
                    .await
            })
        })
    });
}

fn bench_tool_schema_generation(c: &mut Criterion) {
    let mut registry = ToolRegistry::new();
    rustant_tools::register_builtin_tools(&mut registry, std::env::temp_dir());

    c.bench_function("all_tools_parameters_schema", |b| {
        let tools: Vec<Arc<dyn Tool>> = registry
            .list_names()
            .iter()
            .filter_map(|n| registry.get(n))
            .collect();
        b.iter(|| {
            for tool in &tools {
                black_box(tool.parameters_schema());
            }
        })
    });
}

criterion_group!(
    benches,
    bench_tool_registry,
    bench_calculator_tool,
    bench_echo_tool,
    bench_datetime_tool,
    bench_tool_schema_generation,
);
criterion_main!(benches);
