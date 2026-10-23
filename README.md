# Peek

Overlay de desempenho pra Windows 11. Fica na bandeja do sistema e, quando um jogo entra em tela cheia ou em janela sem borda, mostra por cima dele um painel pequeno com FPS, gráfico de frametime, uso e temperatura da GPU e uso e temperatura da CPU. Não injeta DLL, não faz hook de DirectX e não lê a memória do jogo: o FPS vem de eventos ETW do sistema, o uso de GPU e CPU vem de contadores do Windows (PDH), a temperatura da GPU do D3DKMT e a da CPU do driver PawnIO (opcional).

A spec está em `docs/superpowers/specs/2026-10-22-perf-overlay-design.md`. Os nomes internos (crates `overlay-engine` e `perf-overlay`) não mudaram.

## Compilar

```powershell
npm install
npx tauri build
```

O instalador NSIS sai em `target/release/bundle/nsis/` (`Peek_0.1.0_x64-setup.exe`). Ele instala por máquina e pede administrador.

## Desenvolver

- `npm run dev`: só a interface no navegador, com dados simulados (mock).
- `npx tauri dev`: o app completo. O build de debug roda sem administrador, então o que exige admin (FPS por ETW, temperatura da CPU) aparece como indisponível.

Testes:

```powershell
cargo test --workspace
npm test
```

Medições de memória e CPU: `scripts/measure.ps1 -Name Peek -Seconds 20` (processo elevado pro build release). Resultados em `docs/measurements.md`.

## Por que precisa de administrador

A sessão ETW que entrega os eventos de apresentação de quadros (o FPS) e o acesso ao driver PawnIO exigem um processo elevado. O build release traz um manifesto `requireAdministrator`; por isso o Windows mostra o UAC ao abrir. A opção "Iniciar com o Windows" cria uma tarefa agendada chamada "Peek" que sobe o app elevado no logon, sem novo UAC.

## Configurações

Ficam em `%APPDATA%\Peek\settings.json` e são gravadas a cada mudança.

## Limitações

- Tela cheia exclusiva não é suportada: use janela sem borda (ou tela cheia em janela).
- FPS só pra DXGI e D3D9. Jogos Vulkan e OpenGL mostram "—" no FPS por enquanto.
- A detecção do jogo é uma heurística (janela em primeiro plano cobrindo o monitor, ignorando navegadores e o próprio app), então pode errar em casos incomuns.
- A temperatura da CPU depende do driver [PawnIO](https://pawnio.eu/), que não vem no instalador do Peek. Sem ele, a pílula "CPU temp." mostra "requer driver"; clicar nela abre a folha do driver, e "Instalar" baixa o pacote oficial pelo winget (`winget install --id namazso.PawnIO`), o que precisa de internet nesse momento. Também dá pra rodar esse comando à mão.

## Avisos de licença

O executável embute dois módulos assinados do PawnIO, `AMDFamily17.bin` e `IntelMSR.bin`, da release `0.2.11` de [namazso/PawnIO.Modules](https://github.com/namazso/PawnIO.Modules), copiados sem alteração. Eles são distribuídos sob a **GNU LGPL 2.1 ou posterior** (`SPDX-License-Identifier: LGPL-2.1-or-later`); o texto da licença está em <https://github.com/namazso/PawnIO.Modules/blob/0.2.11/COPYING> e o código-fonte (`AMDFamily17.p`, `IntelMSR.p`) na tag [`0.2.11`](https://github.com/namazso/PawnIO.Modules/tree/0.2.11). Detalhes e hashes em `crates/engine/resources/pawnio/README.md`.
