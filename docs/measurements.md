# Medições

Medido com `scripts/measure.ps1` (processo elevado, build release `--no-bundle`; o nome do processo no `-Name` é sem `.exe`: `Peek` no build release, `perf-overlay` no debug).

| Situação | RAM privada | CPU | Data |
|---|---|---|---|
| Base: bandeja apenas, sem janela e sem WebView | 1,6 MB | 0% | 2026-10-22 |

A base ficou abaixo de 20 MB, então o orçamento da spec permanece: oculto < 20 MB e ~0% CPU; overlay visível < 40 MB e < 1% CPU.

## Dado extra: build debug, sem elevação (não é a medição do orçamento)

`target/debug/perf-overlay.exe`, não elevado, 60 s depois de iniciar, `measure.ps1 -Name perf-overlay -Seconds 20`: 10,6 MB privados, 0,09% CPU, 1 filho msedgewebview2 (2026-10-22). Só referência: o build debug não tem o manifesto de administrador e o FPS por ETW fica indisponível.

## Medições do orçamento (build release instalado, elevado): adiado para validação final com o usuário

| Situação | RAM privada | CPU | Data |
|---|---|---|---|
| Oculto (60 s após iniciar) | pendente | pendente | |
| Overlay visível (`fake_game --fps 60`, 30 s) | pendente | pendente | |
| Configuração (abrir, fechar, esperar 15 s) | pendente | pendente | |

Orçamento: oculto < 20 MB e ~0% CPU; overlay visível < 40 MB e < 1% CPU.

## Aceite no The Witcher 3: adiado para o usuário

1. Overlay aparece sozinho em tela cheia ou janela sem borda: pendente.
2. FPS bate com o contador da Steam (±2): pendente.
3. GPU % e temperatura batem com o Gerenciador de Tarefas: pendente.
4. Temperatura da CPU plausível (com o PawnIO instalado): pendente.
5. Alt+Tab esconde e reexibe; Ctrl+Shift+O esconde até fechar o jogo: pendente.
6. FPS médio de 30 s com e sem overlay sem diferença perceptível: pendente.
