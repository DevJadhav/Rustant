# ML Engine

The `rustant-ml` crate provides ML/AI engineering capabilities across the full lifecycle.

## Overview

121 source files, 7,300+ LOC, 33 tests, 54 tool wrappers. Built on four foundational pillars: Safety, Security, Transparency, Interpretability.

## Architecture

### Runtime (`runtime.rs`)
`PythonRuntime` provides subprocess-based Python execution for ML workloads. Isolated execution environment with stdin/stdout communication.

### Tool Macro
The `ml_tool!` macro generates Tool trait implementations:
```
ml_tool!(DataIngest, "ml_data_ingest", "Ingest data from various sources");
```

## Phases

### Phase 1: Data & Features
- **Data**: Sources, schema inference, transforms, validation, storage, lineage tracking
- **Features**: Definition, transforms, feature store, registry

### Phase 2: Training
- Experiment tracking with metadata and reproducibility
- Training runner with configurable callbacks
- Metrics collection and checkpoint management
- Hyperparameter sweep orchestration

### Phase 3: Model Zoo & Algorithms
- Model registry with cards, download, conversion, benchmarking, provenance
- Classical ML algorithms (regression, classification, clustering)
- Neural network support with evaluation and explainability

### Phase 4: LLM Operations
- Fine-tuning pipelines with LoRA/QLoRA adapter support
- Dataset preparation and formatting
- Quantization (GPTQ, GGML, AWQ)
- Evaluation harness for LLM benchmarks
- Alignment (RLHF, DPO) and red teaming

### Phase 5: RAG
- Document ingestion from multiple formats
- Chunking strategies (fixed, semantic, recursive)
- Retrieval with multiple backends
- Reranking for precision
- Context assembly with token budgets
- Grounding verification and diagnostics
- Collection management and evaluation

### Phase 6: Evaluation
- LLM-as-judge evaluation framework
- Error analysis and categorization
- Domain-specific evaluations
- Test case generators
- CI integration for regression detection
- Inter-annotator agreement metrics

### Phase 7: Inference
- Backend support: Ollama, vLLM, llama.cpp, Candle
- Model registry with format management
- Serving configuration and streaming
- Performance profiling

### Phase 8: Research
- Methodology frameworks and templates
- Comparison analysis tools
- Literature review automation
- Dataset discovery and management
- Reproducibility tracking
- Bibliography management
- Notebook generation and synthesis

### Phase 9: Pillar Modules
- **Safety**: Bias detection, fairness metrics, safety evaluation, guardrails
- **Security**: Adversarial robustness, privacy analysis, model security
- **Transparency**: Model documentation, decision explanations, audit trails
- **Interpretability**: Feature importance, attention visualization, concept probing

## Tool Count by Domain

| Domain | Tools | Examples |
|--------|-------|---------|
| Data | 6 | ingest, validate, transform, profile, lineage, export |
| Features | 3 | define, compute, register |
| Training | 5 | experiment, train, evaluate, checkpoint, sweep |
| Zoo | 5 | search, download, convert, benchmark, card |
| LLM | 5 | finetune, prepare, quantize, evaluate, align |
| RAG | 5 | ingest, query, evaluate, diagnose, manage |
| Eval | 4 | judge, analyze, generate, benchmark |
| Inference | 4 | serve, profile, compare, optimize |
| Research | 4 | review, compare, reproduce, synthesize |
| Safety | 4 | bias, fairness, safety_eval, guardrail |
| Security | 3 | adversarial, privacy, model_sec |
| Transparency | 3 | document, explain, audit |
| Interpretability | 3 | importance, attention, probe |

## Integration

- 6 new `ActionDetails` variants: ModelInference, ModelTraining, DataPipeline, RagQuery, EvaluationRun, ResearchAction
- 4 new `DecisionType` variants: ModelSelection, RetrievalStrategy, SafetyOverride, EvaluationJudgement
- 16 slash commands in AiEngineer category
- 8 workflow templates (ml_training, rag_pipeline, model_evaluation, etc.)
- Agent routing: `ml_tool_routing_hint()` for ML-specific tasks
