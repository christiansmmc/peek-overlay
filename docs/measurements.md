# Medições

Medido com `scripts/measure.ps1` (processo elevado, build release `--no-bundle`; o nome do processo no `-Name` é sem `.exe`: `Peek` no build release, `perf-overlay` no debug).

| Situação | RAM privada | CPU | Data |
|---|---|---|---|
| Base: bandeja apenas, sem janela e sem WebView | 1,6 MB | 0% | 2026-10-22 |

A base ficou abaixo de 20 MB, então o orçamento da spec permanece: oculto < 20 MB e ~0% CPU; overlay visível < 40 MB e < 1% CPU.

## Dado extra: build debug, sem elevação (não é a medição do orçamento)

`target/debug/perf-overlay.exe`, não elevado, 60 s depois de iniciar, `measure.ps1 -Name perf-overlay -Seconds 20`: 10,6 MB privados, 0,09% CPU, 1 filho msedgewebview2 (2026-10-22). Só referência: o build debug não tem o manifesto de administrador e o FPS por ETW fica indisponível.

## Dado extra: custo do NVML (uso de GPU NVIDIA), build debug, sem elevação

O controlador rodando sem o Tauri (oculto 30 s, overlay visível 35 s, oculto de novo 35 s), `measure.ps1 -Seconds 15` em cada fase, RTX 4070 Ti, driver 617.42 (2026-10-22):

| Fase | Antes (PDH) | Depois (NVML) |
|---|---|---|
| Oculto, logo após iniciar | 8,4 MB, 0% | 7,4 MB, 0% |
| Overlay visível | 9,0 MB, 0,06% | 27,7 MB, 0,1% |
| Oculto de novo | 9,0 MB, 0% | 9,2 MB, 0,02% |

O NVML ocupa uns 19 MB enquanto está carregado (a seção de dados do `nvml.dll` do driver). Por isso ele só é carregado enquanto há amostragem e é liberado quando ela para; carregado pelo `System32\nvml.dll`, esses 19 MB não voltam nem depois de `nvmlShutdown`.

## Medições do orçamento (build release instalado, elevado)

Validadas com o usuário em 2026-10-22, instalador silencioso em `C:\Program Files\Peek`.

| Situação | RAM privada | CPU | Data |
|---|---|---|---|
| Oculto na bandeja (primeiro build, antes do NVML) | 11,2 MB, 0 WebView2 | 0% | 2026-10-22 |
| Overlay visível sobre o `fake_game --fps 60` (antes do NVML) | 12,3 MB | 0,2% | 2026-10-22 |
| Configuração aberta → fechada, 15 s depois | 12,6 MB, 0 WebView2 | 0% | 2026-10-22 |
| Oculto na bandeja (build final, com NVML) | 8,1 MB | 0% | 2026-10-22 |
| Overlay visível sobre o The Witcher 3 (build final, NVML carregado) | 32,1 MB | 0,22% | 2026-10-22 |

Orçamento: oculto < 20 MB e ~0% CPU; overlay visível < 40 MB e < 1% CPU. Tudo dentro.

## Aceite no The Witcher 3 (usuário, janela sem bordas, RTX 4070 Ti)

1. Overlay aparece sozinho em tela cheia ou janela sem borda: ok.
2. FPS bate com o contador da Steam (±2): ok.
3. GPU % bate com o overlay da Steam: ok depois da troca pra NVML (antes o PDH dava 66–80% contra 96% da Steam); temperaturas plausíveis: ok.
4. Temperatura da CPU plausível (PawnIO 2.2.0 instalado): ok.
5. Alt+Tab esconde e reexibe; Ctrl+Shift+O esconde até fechar o jogo: ok.
6. FPS médio com e sem overlay sem diferença perceptível: ok.

Outros testes da validação final: cenário do `engine_demo` elevado com o jogo falso (aparece sozinho, some quando o jogo fecha), primeira execução (abre a janela, grava as configurações, cria a tarefa "Peek"), clique e menu da bandeja, arrastar a janela, atalho, "Iniciar com o Windows" remove/recria a tarefa, "Sair" encerra, desinstalador remove o app e a tarefa.
