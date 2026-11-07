# AI on chip vs a local language/runtime

This is a reality check for the idea "a language that controls the hardware can
run a 27B model with 1M context at 10,000 tokens/s on a laptop." Some parts are
real; the specific target is not physically supportable on this machine.

## The machine

Measured 2026-11-07 on the development host:

- AMD Ryzen AI 7 350, 8 cores / 16 threads, Radeon 860M integrated graphics
- **30 GiB RAM total**, ~19 GiB available, 8 GiB swap
- Radeon 860M shares system RAM; no discrete GPU (`nvidia-smi` absent)
- AMD XDNA/Krackan NPU present (`lspci`: Strix/Krackan Neural Processing Unit)
- No independent HBM/VRAM pool

So every weight and every KV-cache byte competes for the same ~30 GiB.

## DeepSeek V4.1 Flash 1017 is real

The model is real (released 2026-10-17). Cloudflare's Workers AI changelog
lists a **552B-parameter MoE backbone**, **8B active during prefill**, **16B
active during decode**, and a **1,048,576-token context**. A direct Hugging
Face `config.json` fetch failed here on DNS; the Cloudflare spec is the
authoritative secondary source used below.

| precision | 552B total weights | fits 30 GiB? |
|---|---:|---|
| FP16 | ~1,104 GB | no |
| 8-bit | ~552 GB | no |
| 4-bit | ~276 GB | no |
| 2-bit | ~138 GB | no |
| 1-bit | ~69 GB | no |
| 0.43-bit | ~30 GB | only by destroying quality |

Even with only 16B active parameters per decode token, 4-bit active weights
are ~8 GB/token: at 100 GB/s that is a ~12.5 tok/s ceiling before attention,
KV cache, routing and OS overhead. To reach 10,000 tok/s single-stream, the
active working set would have to be ~0.08 GB/token (about 0.04 bit/parameter).
That is not a language problem; it is bytes-per-token and memory bandwidth.

What can still be researched: 1-bit/ternary weights, expert offload and
caching, KV quantisation/paging, speculative decoding, distillation, and
distribution across devices. Any of these that keeps quality while cutting
bytes is valuable; none of them is created by adding syntax to a language.

## Why on-chip AI is fast

Accelerators such as Groq, Cerebras, Etched Sohu and Taalas win by changing
where weights and activations live:

- Groq/Cerebras use large on-chip SRAM and deterministic dataflow to avoid
  DRAM round-trips.
- Etched's Sohu hard-wires transformer operations; its 2024 "500,000 tok/s
  Llama-70B" and "160 H100s" numbers are vendor claims without published test
  conditions or independent benchmarks (see the [2026 review](https://www.aquanode.io/blog/etched-sohu-vs-nvidia)).
- Taalas-class designs bake model weights into silicon; they trade
  programmability and model freshness for bandwidth/latency.

None of this changes the bytes a model must read. A language can improve
layout, fusion, scheduling and quantisation; it cannot create memory capacity
or bandwidth that the package does not have.

## Arithmetic for this host

Assumptions: 4-bit weights (0.5 byte/parameter), ~100 GB/s effective shared
memory bandwidth (an optimistic order of magnitude for this laptop class), and
one token per weight read for a dense model.

| model | weights at 4-bit | bandwidth-bound single-stream ceiling |
|---|---:|---:|
| 27B dense | 13.5 GB | ~7.4 tok/s |
| 30B MoE, 3B active | 15 GB total, ~1.5 GB active/token | ~67 tok/s upper bound; 20-40 realistic |
| 671B MoE | 335 GB | does not fit |

At 10,000 tok/s, one token would have to read 13.5 GB in 0.1 ms = 135 TB/s.
That is three orders of magnitude beyond this package. Batching amortises
weight reads, but it still cannot make a single 27B stream 10k tok/s; it raises
aggregate throughput and latency.

### KV cache for 1M tokens

KV bytes = 2 (K and V) × layers × KV heads × head dim × bytes × tokens.

| config | FP16 KV for 1M tokens | 4-bit KV |
|---|---:|---:|
| 48 layers, 4 KV heads, 128 dim | ~98 GB | ~25 GB |
| 64 layers, 8 KV heads, 128 dim | ~262 GB | ~66 GB |
| 80 layers, 8 KV heads, 128 dim | ~328 GB | ~82 GB |

A 27B 4-bit model (13.5 GB) plus even the smallest 4-bit 1M KV (25 GB) already
exceeds 30 GiB before activations, OS and fragmentation. Long context requires
paging/offload, KV quantisation, sparse attention, or a smaller context; each
costs speed and/or quality.

## What is achievable on this host

These are engineering targets, not promises:

- **7B-class, 4-bit, 8k context**: 10-30 tok/s single stream, usable chat;
  memory ~5-6 GB. This is the realistic first demo.
- **27B-class, 4-bit, 4-8k context**: ~3-8 tok/s, memory ~15-18 GB; batch
  throughput can be higher, but latency is not API-fast.
- **30B MoE, 3B active, 4-bit**: potentially 20-40 tok/s single stream if the
  runtime streams only active experts and keeps the rest quantised; 30 GB is
  tight but possible with expert offload.
- **1M context**: possible only with paged/quantised KV and a much smaller
  model, or as an offline experiment with heavy offload. Not a 10k tok/s
  laptop demo.
- **10,000 tok/s**: plausible only as aggregate batch throughput on an
  accelerator, or for a tiny model/short context. Not for a single 27B stream.

## What MAGIC should build

Do not add `ai` keywords. Build a runtime/compiler stack that owns layout:

1. **Tensor type with rank/shape checks** and explicit dtype/layout.
2. **Memory planner** for weights, activations and paged KV; report the
   worst-case bound (the actual MAGIC differentiator).
3. **Quantised kernels** (4/8-bit) and fused attention/MLP; start CPU, then
   NPU/GPU via C ABI.
4. **Expert streaming** for MoE; measure bytes/token and tok/s.
5. **WASM sandbox** with a hard heap cap: a 5 KB plugin with a verified budget
   is a real product even if it cannot run 27B.
6. **FFI first**, not CUDA/Triton from scratch: call BLAS/GGML/ORT, then
   replace hot kernels.

The measurable first milestone is not 10k tok/s. It is: **a 7B 4-bit model,
8k context, 10+ tok/s on this laptop, with a printed memory bound that is
greater than or equal to measured peak RSS.** That is useful, honest, and a
platform for the harder work.

## Sources and confidence

- Host specs: local `lscpu`, `free -h`, `lspci` (VERIFIED).
- Memory/bandwidth arithmetic: this document's formulas (INFERENCE; effective
  bandwidth must be measured, not assumed).
- Etched claims/status: [Aquanode review](https://www.aquanode.io/blog/etched-sohu-vs-nvidia)
  (secondary source; treats 2024 numbers as vendor claims).
- Goose language facts: [aardappel/goose](https://github.com/aardappel/goose)
  and its [benchmark results](https://github.com/aardappel/goose/blob/master/bench/results.md).

The 10k tok/s target as stated is not achievable on this hardware. The useful
target is API-comparable latency for small/quantised models, with a proven
memory budget, and aggregate throughput for batch workloads.
