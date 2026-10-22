# Medições

Medido com `scripts/measure.ps1` (processo elevado, build release `--no-bundle`).

| Situação | RAM privada | CPU | Data |
|---|---|---|---|
| Base: bandeja apenas, sem janela e sem WebView | 1,6 MB | 0% | 2026-10-22 |

A base ficou abaixo de 20 MB, então o orçamento da spec permanece: oculto < 20 MB e ~0% CPU; overlay visível < 40 MB e < 1% CPU.
