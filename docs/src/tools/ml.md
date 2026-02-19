# ML / AI Engineer Tools

The `rustant-ml` crate provides 54 tools for the full AI/ML engineering lifecycle: data engineering, feature management, model training, evaluation, inference, and research. All tools are built on four foundational pillars: **Safety**, **Security**, **Transparency**, and **Interpretability**.

## Tool Summary by Domain

| Domain | Tools | Description |
|--------|-------|-------------|
| Data | 6 | Ingestion, transformation, validation, splitting, versioning, export |
| Feature | 3 | Definition, computation, serving |
| Training | 5 | Training runs, experiments, hyperparameters, checkpoints, metrics |
| Zoo | 5 | Model registry, download, convert, serve, benchmark |
| LLM | 5 | Fine-tuning, dataset prep, quantization, evaluation, adapters |
| RAG | 5 | Ingestion, query, collections, chunking, pipelines |
| Eval | 4 | Benchmarks, LLM-as-judge, error analysis, reports |
| Inference | 4 | Serve, stop, status, benchmark |
| Research | 4 | Literature review, comparison, reproducibility, bibliography |
| Safety | 4 | Safety checks, PII scanning, bias detection, alignment testing |
| Security | 3 | Red teaming, adversarial scanning, provenance verification |
| Transparency | 3 | Decision explanation, data lineage, source attribution |
| Interpretability | 3 | Attention analysis, feature importance, counterfactuals |

**Total: 54 tools**

---

## Tool Implementation Pattern

All ML tools use the `ml_tool!` macro for consistent implementation:

```rust
ml_tool!(ToolStruct, "tool_name", "description", RiskLevel::Write,
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {"type": "string", "enum": [...], "description": "Action"},
            // parameters...
        },
        "required": ["action"]
    })
);
```

Each tool holds an `Arc<PathBuf>` workspace reference and stores state in `.rustant/ml/`.

---

## Data Tools (6)

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `ml_data_ingest` | Write | Ingest data from CSV, JSON, JSONL, SQLite, or API sources |
| `ml_data_transform` | Write | Apply transformations (clean, normalize, encode, augment) |
| `ml_data_validate` | Read-only | Validate quality with PII scanning, drift detection, quality gates |
| `ml_data_split` | Write | Split datasets (train/test, stratified, k-fold, temporal) |
| `ml_data_version` | Read-only | Version control with content hashing and diff |
| `ml_data_export` | Write | Export to CSV, JSON, or Parquet format |

### ml_data_ingest

| Action | Description |
|--------|-------------|
| `ingest` | Load data from a source |
| `preview` | Preview first N rows |
| `schema` | Infer and display data schema |
| `stats` | Compute descriptive statistics |

### ml_data_validate

| Action | Description |
|--------|-------------|
| `quality_report` | Generate a data quality report |
| `check_drift` | Detect distribution drift between datasets |
| `pii_scan` | Scan for personally identifiable information |

---

## Feature Tools (3)

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `ml_feature_define` | Write | Define, list, show, or delete feature definitions |
| `ml_feature_compute` | Execute | Compute features from raw data using defined transforms |
| `ml_feature_serve` | Read-only | Serve features for online inference or batch retrieval |

---

## Training Tools (5)

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `ml_train` | Execute | Start, stop, or monitor model training runs |
| `ml_experiment` | Write | Create, list, compare, or explain training experiments |
| `ml_hyperparams` | Execute | Run hyperparameter sweeps (grid, random, Bayesian) |
| `ml_checkpoint` | Read-only | List, load, compare, or export training checkpoints |
| `ml_metrics` | Read-only | Plot, compare, and analyze metrics with anomaly detection |

---

## Model Zoo Tools (5)

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `ml_model_registry` | Write | List, add, remove, or search models with ModelCard support |
| `ml_model_download` | Execute | Download from HuggingFace, Ollama, or URL with provenance |
| `ml_model_convert` | Execute | Convert between ONNX, CoreML, GGUF, and TFLite formats |
| `ml_model_serve` | Execute | Start or stop model serving with health monitoring |
| `ml_model_benchmark` | Execute | Benchmark latency, throughput, accuracy, and safety |

---

## LLM Tools (5)

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `ml_finetune` | Execute | Fine-tune LLMs with LoRA/QLoRA/full methods and alignment checking |
| `ml_chat_dataset` | Write | Create, convert, validate, and PII-scan chat fine-tuning datasets |
| `ml_quantize` | Execute | Quantize models with GPTQ, AWQ, GGUF, or BitsAndBytes |
| `ml_eval` | Execute | Run LLM benchmarks (perplexity, MMLU, HumanEval, safety) |
| `ml_adapter` | Write | Manage LoRA adapters -- list, merge, switch, delete with provenance |

---

## RAG Tools (5)

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `rag_ingest` | Write | Ingest documents with PII scanning and lineage tracking |
| `rag_query` | Read-only | Query with source attribution and groundedness checking |
| `rag_collection` | Write | Create, list, delete, and manage document collections |
| `rag_chunk` | Read-only | Preview and configure document chunking strategies |
| `rag_pipeline` | Write | Configure, test, and evaluate end-to-end RAG pipelines |

---

## Evaluation Tools (4)

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `eval_benchmark` | Execute | Run evaluation benchmarks with safety benchmarks |
| `eval_judge` | Execute | LLM-as-Judge with configurable rubrics and bias correction |
| `eval_analyze` | Read-only | Automated error taxonomy, distribution, saturation detection |
| `eval_report` | Read-only | Generate reports with trend analysis and audit trails |

---

## Inference Tools (4)

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `inference_serve` | Execute | Serve models with resource limits and output validation |
| `inference_stop` | Execute | Stop running inference instances |
| `inference_status` | Read-only | List running instances and health information |
| `inference_benchmark` | Execute | Benchmark latency, throughput, and cost |

Supported backends: Ollama, vLLM, llama.cpp, Candle.

---

## Research Tools (4)

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `research_review` | Read-only | Automated literature review with synthesis and gap analysis |
| `research_compare` | Read-only | Compare papers and methodologies side-by-side |
| `research_repro` | Write | Track reproducibility attempts with environment snapshots |
| `research_bibliography` | Read-only | Export references in BibTeX, RIS, or CSL-JSON format |

---

## Pillar Tools

### Safety (4)

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `ai_safety_check` | Read-only | Check model outputs, datasets, and configs for safety issues |
| `ai_pii_scan` | Read-only | Scan text, files, or datasets for PII with optional redaction |
| `ai_bias_detect` | Read-only | Analyze models and datasets for demographic biases |
| `ai_alignment_test` | Execute | Test alignment for harmlessness, helpfulness, and honesty |

### Security (3)

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `ai_red_team` | Execute | Generate adversarial attacks and run red team campaigns |
| `ai_adversarial_scan` | Read-only | Scan for adversarial manipulation, jailbreaks, injection |
| `ai_provenance_verify` | Read-only | Verify model and dataset provenance, integrity, supply chain |

### Transparency (3)

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `ai_explain_decision` | Read-only | Explain agent decisions with full reasoning chains |
| `ai_data_lineage` | Read-only | Trace data and model lineage with graph visualization |
| `ai_source_attribution` | Read-only | Attribute claims to sources with confidence scoring |

### Interpretability (3)

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `ai_attention_analyze` | Execute | Extract and visualize attention patterns in transformers |
| `ai_feature_importance` | Execute | Compute feature importance via SHAP, LIME, or permutation |
| `ai_counterfactual` | Read-only | Generate counterfactual explanations for input-output relationships |

---

## Agent Integration

ML tools add 6 `ActionDetails` variants to the safety system:
- `ModelInference` -- model prediction requests
- `ModelTraining` -- training run execution
- `DataPipeline` -- data processing operations
- `RagQuery` -- retrieval-augmented generation queries
- `EvaluationRun` -- benchmark and evaluation execution
- `ResearchAction` -- research workflow actions

And 4 `DecisionType` variants:
- `ModelSelection` -- choosing models
- `RetrievalStrategy` -- RAG retrieval configuration
- `SafetyOverride` -- overriding safety checks (requires approval)
- `EvaluationJudgement` -- evaluation scoring decisions

---

## Related REPL Commands

ML tools are accessible via 16 slash commands in the `AiEngineer` category:

| Command | Description |
|---------|-------------|
| `/data` | Data pipeline operations |
| `/features` | Feature engineering |
| `/train` | Model training |
| `/zoo` | Model zoo management |
| `/finetune` | LLM fine-tuning |
| `/rag` | RAG pipeline |
| `/eval` | Evaluation benchmarks |
| `/inference` | Model serving |
| `/mlresearch` | ML research workflows |
| `/safety` | AI safety checks |
| `/redteam` | Red team campaigns |
| `/lineage` | Data lineage |
| `/explain` | Decision explanation |
| `/bias` | Bias detection |
| `/pii` | PII scanning |
| `/provenance` | Provenance verification |

## Related Workflow Templates

8 ML workflow templates are available:
- `ml_experiment`, `ml_finetune`, `ml_rag_pipeline`, `ml_evaluation`
- `ml_safety_audit`, `ml_data_pipeline`, `ml_model_deploy`, `ml_research`
