# Overlay de desempenho — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** App de bandeja para Windows 11 que mostra, por atalho ou ao detectar um jogo, um overlay leve com FPS, frametime, uso e temperatura de GPU e CPU, com a tela de configuração do handoff rodando em WebView2.

**Architecture:** Workspace Cargo com dois crates. `crates/engine` (`overlay-engine`) tem toda a lógica e a integração com o Windows (configurações, atalho, ETW, sensores, PawnIO, detecção de jogo, overlay em Direct2D e o orquestrador `Controller`) e não depende do Tauri, então roda com `cargo test` e tem exemplos executáveis. `src-tauri` (`perf-overlay`) é a casca Tauri v2: bandeja, instância única, janela de configuração, comandos e eventos. O frontend fica em `src/` (Preact + TypeScript + Vite) e tem um backend simulado pra desenvolver no navegador.

**Tech Stack:** Rust 1.96 (msvc), Tauri 2, crates `windows`, `ferrisetw`, `libloading`, `serde`/`serde_json`; Node 24 + npm, Vite, Preact, TypeScript, Vitest.

**Spec:** `docs/superpowers/specs/2026-10-22-perf-overlay-design.md`. Design da tela: `docs/design-handoff/README.md` e `docs/design-handoff/Configuracoes Overlay.dc.html`.

## Global Constraints

- Só Windows 11 x64. Toolchain `stable-x86_64-pc-windows-msvc` (rustc 1.96), Node 24, npm. Comandos rodam na raiz `C:\Users\csequ\projects\perf-overlay`.
- Sem injeção, sem hook de DirectX, sem leitura de memória de outro processo. O processo do app **nunca** cria dispositivo D3D; só o exemplo `fake_game` cria.
- Crate `windows`: use a versão que o Tauri já puxa (`cargo tree -i windows -p perf-overlay`) e ative só as features que cada módulo usa.
- UI 100% offline: CSP `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:`.
- Textos em pt-BR exatamente como no handoff; decimal com vírgula ("6,9 ms").
- Release: manifesto com `requireAdministrator`, `PerMonitorV2`, Common Controls v6. Debug: `asInvoker`; o que exige admin aparece como indisponível.
- Configurações em `%APPDATA%\perf-overlay\settings.json`, formato da spec 4.7.
- Cores do overlay (sRGB, convertidas do oklch do handoff): rótulo GPU `#B6A8FF`; rótulo CPU e linha do gráfico `#73BDFF`; barra GPU `#A492FB`; barra CPU `#52A9FE`; trilho da barra branco 14%; fundo `rgb(12,12,18)` com alpha = opacidade; texto `#FFFFFF`; texto secundário branco 65%; sombra do texto preto 85% deslocada (0, 1) px, sem blur.
- Orçamento (spec 6): oculto < 20 MB privados e ~0% CPU; overlay visível < 40 MB e < 1% CPU; medido com `scripts/measure.ps1`.
- FPS só via DXGI e D3D9 neste plano. Vulkan/OpenGL mostram "—" (a spec 4.4 aceita) e ficam registrados no README como limitação.
- Passos **[ADMIN]** precisam de processo elevado: use `scripts/run-elevated.ps1` (Task 6). O usuário clica "Sim" no UAC.
- **Nunca** instale o PawnIO na máquina sem perguntar ao usuário antes.
- Commits em inglês, Conventional Commits, terminando com a linha `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Múltiplos monitores com DPI diferente** (incluindo monitor à esquerda, com coordenadas negativas) e troca de resolução: o overlay fica no monitor do jogo, na escala daquele monitor, e se reposiciona em `WM_DISPLAYCHANGE`. → testes `place_on_left_monitor_with_150_percent_dpi` (Task 10) e passo em Task 11.
2. **Jogo protegido por anti-cheat, em que `OpenProcess` falha:** o nome cai pra título da janela, depois nome do exe, depois "Jogo". Se não dá pra esperar no handle do processo, o fim do jogo é detectado com `IsWindow(hwnd)` na troca de foco. → teste `display_name_fallback_chain` (Task 9) e passo em Task 12.
3. **Sessão ETW órfã depois de um crash** ("PerfOverlay-Frames" já existe): o próximo start derruba a antiga e começa de novo. → passo [ADMIN] em Task 6.
4. **Atalho já ocupado na inicialização:** o app sobe mesmo assim, o overlay continua acessível pela bandeja e pelo chip de status, e a tela mostra a mensagem de conflito no campo do atalho. → teste `startup_hotkey_error_message` (Task 12) e passo em Task 15.
5. **Windows em português:** os contadores PDH têm nomes localizados; use sempre `PdhAddEnglishCounterW`. → passo de verificação em Task 7 nesta máquina.

---

## Estrutura de arquivos

```
Cargo.toml                      workspace (members: crates/engine, src-tauri) + profile.release
package.json, vite.config.ts, tsconfig.json, index.html
src/                            frontend Preact
  main.tsx, App.tsx, types.ts, ipc.ts, mock.ts, format.ts, keys.ts, theme.css
  components/ TitleBar, StatusRow, Preview, OverlayLine, OverlayCard, MetricPills,
              Segmented, Slider, ShortcutField, GeneralToggles, DriverSheet, Toast (.tsx)
crates/engine/
  src/lib.rs                    declara módulos + macro trace!
  src/settings.rs               Settings, defaults, normalize, load/save, patch
  src/hotkey.rs                 parse de combinação, conflitos conhecidos, mensagens
  src/model.rs                  tipos trocados com a UI (AppState, MetricsSample...)
  src/effects.rs                o que precisa rodar (ETW, timers) + métricas efetivas
  src/game.rs                   máquina de estados da detecção de jogo
  src/process_info.rs           nome do exe, FileDescription, lista de ignorados
  src/autostart.rs              tarefa do Agendador
  src/controller.rs             orquestrador
  src/frames/{mod,clock,stats,etw}.rs
  src/sensors/{mod,cpu_usage,gpu_usage,gpu_temp,adapter,cpu_temp}.rs
  src/pawnio/{mod,install}.rs
  src/overlay/{mod,colors,format,layout,position,render,window}.rs
  src/native/{mod,foreground,process_watch}.rs
  resources/pawnio/{AMDFamily17.bin,IntelMSR.bin,README.md}
  examples/{fake_game,frames_probe,sensors_probe,overlay_demo,engine_demo,hold_hotkey}.rs
src-tauri/
  src/{main,lib,commands,tray,config_window,sink}.rs
  manifests/{release,debug}.manifest, capabilities/default.json, tauri.conf.json, build.rs
scripts/{measure,run-elevated,screenshot}.ps1
docs/measurements.md
```

---

### Task 1: Esqueleto do workspace, casca Tauri só com bandeja e medição da base

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `src/main.tsx`
- Create: `crates/engine/Cargo.toml`, `crates/engine/src/lib.rs`
- Create (via `tauri init`, depois editar): `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/build.rs`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`
- Create: `src-tauri/manifests/release.manifest`, `src-tauri/manifests/debug.manifest`, `scripts/measure.ps1`, `docs/measurements.md`

**Interfaces:**
- Produces: crate `overlay-engine` (lib `overlay_engine`) com `pub fn version() -> &'static str` e a macro `trace!` (igual a `eprintln!` em debug, vazia em release); crate `perf-overlay` (lib `perf_overlay_lib`) com `pub fn run()`.

- [ ] **Step 1: Frontend mínimo.** `npm init -y`; `npm install preact`; `npm install -D vite @preact/preset-vite typescript vitest @tauri-apps/cli@^2 @tauri-apps/api@^2`. Scripts do `package.json`: `"dev": "vite"`, `"build": "tsc --noEmit && vite build"`, `"test": "vitest run"`, `"tauri": "tauri"`. `vite.config.ts`: preset Preact, `server: { port: 1420, strictPort: true }`, `clearScreen: false`, `build.target: 'es2022'`. `index.html` com `lang="pt-BR"`; `src/main.tsx` renderiza o texto "Overlay de desempenho".
- [ ] **Step 2: Tauri.** `npx tauri init --ci --app-name perf-overlay --window-title "Overlay de desempenho" --frontend-dist ../dist --dev-url http://localhost:1420 --before-dev-command "npm run dev" --before-build-command "npm run build"`.
- [ ] **Step 3: Workspace.** Raiz `Cargo.toml`: `[workspace] members = ["crates/engine", "src-tauri"]`, `resolver = "2"`, `[profile.release] lto = true, codegen-units = 1, strip = true`. `src-tauri/Cargo.toml`: deps `tauri = { version = "2", features = ["tray-icon"] }`, `tauri-plugin-single-instance = "2"`, `serde`, `serde_json`, `overlay-engine = { path = "../crates/engine" }`. `.gitignore`: `node_modules/`, `dist/`, `target/`.
- [ ] **Step 4: `tauri.conf.json`.** `productName: "Overlay de desempenho"`, `identifier: "com.perfoverlay.app"`, `app.windows: []` (nenhuma janela no início), `app.security.csp` = CSP das Global Constraints.
- [ ] **Step 5: Manifestos.** `release.manifest`:

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency><dependentAssembly>
    <assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0"
      processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/>
  </dependentAssembly></dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges>
    <requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>
  </requestedPrivileges></security></trustInfo>
  <application xmlns="urn:schemas-microsoft-com:asm.v3"><windowsSettings>
    <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
  </windowsSettings></application>
</assembly>
```

`debug.manifest` é idêntico, mas com `level="asInvoker"`.
- [ ] **Step 6: `build.rs`.** Escolhe o manifesto por `std::env::var("PROFILE")` (`"release"` → release, senão debug) e passa via `tauri_build::WindowsAttributes::new().app_manifest(...)` em `tauri_build::try_build(...)`.
- [ ] **Step 7: `run()`.** Em `src-tauri/src/lib.rs`: plugin single-instance (callback só chama `trace!` por enquanto); no `setup`, uma bandeja (`TrayIconBuilder`, ícone padrão, tooltip "Overlay de desempenho") com menu de um item "Sair" que chama `app.exit(0)`. Em `.run(|app, event| ...)`, trate `RunEvent::ExitRequested { code: None, api, .. }` com `api.prevent_exit()`: fechar a última janela não encerra o app.
- [ ] **Step 8: Teste de fumaça do engine.** Em `crates/engine/src/lib.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn version_is_set() { assert!(!super::version().is_empty()); }
}
```

Run: `cargo test -p overlay-engine` → Expected: PASS (1 test).
- [ ] **Step 9: Build debug e instância única.** Run: `cargo build -p perf-overlay`, depois `Start-Process target\debug\perf-overlay.exe` duas vezes e `(Get-Process perf-overlay).Count`. Expected: `1`. Encerre pelo menu da bandeja (peça ao usuário) ou com `Stop-Process -Name perf-overlay`.
- [ ] **Step 10: `scripts/measure.ps1`.** Parâmetros `-Name <processo>` e `-Seconds <n>`. Amostra 1×/s via `Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "Name='<Name>'"`: `WorkingSetPrivate` e `PercentProcessorTime` ÷ número de processadores lógicos. Conta os `msedgewebview2.exe` cujo `ParentProcessId` (via `Win32_Process`) é o PID do processo. Imprime `private_ws_mb=<max> cpu_pct_avg=<média> webview_children=<n>`. As propriedades WMI são em inglês mesmo no Windows em português.
- [ ] **Step 11 [ADMIN]: Medir a base.** `npx tauri build --no-bundle`; `Start-Process target\release\perf-overlay.exe` (UAC); espere 10 s; `scripts\measure.ps1 -Name perf-overlay -Seconds 10`. Anote em `docs/measurements.md` (tabela: situação, RAM privada, CPU, data). **Regra da spec:** se a base passar de 20 MB, o orçamento vira oculto = base + 5 MB e visível = oculto + 20 MB. Escreva os novos números no arquivo e avise o usuário ao fim da task. Peça ao usuário pra sair pelo menu da bandeja.
- [ ] **Step 12: Commit.** `git add -A && git commit -m "chore: scaffold tauri workspace with tray-only shell"`

---

### Task 2: Modelo do atalho

**Files:**
- Create: `crates/engine/src/hotkey.rs`
- Modify: `crates/engine/src/lib.rs` (`pub mod hotkey;`)

**Interfaces:**
- Produces:
  - `pub const MOD_ALT: u32 = 0x1; MOD_CONTROL = 0x2; MOD_SHIFT = 0x4; MOD_WIN = 0x8; MOD_NOREPEAT = 0x4000;`
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub struct Hotkey { pub mods: u32, pub vk: u32 }`
  - `#[derive(Clone, Debug, PartialEq, Eq)] pub enum HotkeyError { NoModifier, UnknownKey(String), KnownConflict { app: &'static str }, InUse }`
  - `pub fn parse(combo: &[String]) -> Result<Hotkey, HotkeyError>`: `mods` sempre inclui `MOD_NOREPEAT`.
  - `pub fn known_conflict(combo: &[String]) -> Option<&'static str>`
  - `pub fn display(combo: &[String]) -> String`: junta com `" + "`.
  - `impl HotkeyError { pub fn message(&self, combo: &[String]) -> String }`

Nomes aceitos (são os que `keys.ts`, na Task 15, produz): modificadores `Ctrl`, `Alt`, `Shift`, `Win`; `A`–`Z` → 0x41–0x5A; `0`–`9` → 0x30–0x39; `F1`–`F24` → 0x70+; `Num 0`–`Num 9` → 0x60+; `Num Multiply` 0x6A, `Num Add` 0x6B, `Num Subtract` 0x6D, `Num Decimal` 0x6E, `Num Divide` 0x6F, `Num Enter` 0x0D; `Espaço` 0x20, `Insert` 0x2D, `Home` 0x24, `End` 0x23, `PgUp` 0x21, `PgDn` 0x22, `Del` 0x2E, `↑` 0x26, `↓` 0x28, `←` 0x25, `→` 0x27, `Tab` 0x09, `Enter` 0x0D, `Backspace` 0x08, `Esc` 0x1B, `` ` `` 0xC0, `-` 0xBD, `=` 0xBB, `[` 0xDB, `]` 0xDD, `\` 0xDC, `;` 0xBA, `'` 0xDE, `,` 0xBC, `.` 0xBE, `/` 0xBF. Conflitos conhecidos (copiados do protótipo): `Ctrl+Shift+M` Discord, `Alt+Z` NVIDIA App, `Alt+R` AMD Software, `Win+G` Xbox Game Bar, `Ctrl+Shift+Esc` Windows, `Ctrl+Shift+S` Steam, `Shift+Tab` Steam.

- [ ] **Step 1: Escrever os testes**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn s(v: &[&str]) -> Vec<String> { v.iter().map(|x| x.to_string()).collect() }

    #[test]
    fn parses_default_combo() {
        let hk = parse(&s(&["Ctrl", "Shift", "O"])).unwrap();
        assert_eq!(hk.vk, 0x4F);
        assert_eq!(hk.mods, MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT);
    }
    #[test]
    fn parses_named_keys() {
        for (name, vk) in [("Espaço", 0x20), ("F12", 0x7B), ("Num 7", 0x67), ("PgUp", 0x21),
                           ("↑", 0x26), ("`", 0xC0), ("-", 0xBD), ("=", 0xBB), ("Del", 0x2E), ("5", 0x35)] {
            assert_eq!(parse(&s(&["Alt", name])).unwrap().vk, vk, "{name}");
        }
    }
    #[test]
    fn rejects_combo_without_modifier() {
        assert_eq!(parse(&s(&["O"])), Err(HotkeyError::NoModifier));
    }
    #[test]
    fn rejects_unknown_key() {
        assert!(matches!(parse(&s(&["Ctrl", "Ç"])), Err(HotkeyError::UnknownKey(_))));
    }
    #[test]
    fn known_conflicts_from_prototype() {
        assert_eq!(known_conflict(&s(&["Ctrl", "Shift", "M"])), Some("Discord"));
        assert_eq!(known_conflict(&s(&["Shift", "Tab"])), Some("Steam"));
        assert_eq!(known_conflict(&s(&["Ctrl", "Shift", "O"])), None);
    }
    #[test]
    fn messages_in_pt_br() {
        assert_eq!(display(&s(&["Ctrl", "Alt", "P"])), "Ctrl + Alt + P");
        assert_eq!(HotkeyError::KnownConflict { app: "Discord" }.message(&s(&["Ctrl", "Shift", "M"])),
                   "Ctrl + Shift + M já é usado por Discord. Escolha outra.");
        assert_eq!(HotkeyError::InUse.message(&s(&["Ctrl", "Shift", "O"])),
                   "Ctrl + Shift + O já é usado por outro app. Escolha outra.");
        assert_eq!(HotkeyError::NoModifier.message(&s(&["O"])),
                   "Use pelo menos Ctrl, Alt ou Shift junto com a tecla.");
        assert_eq!(HotkeyError::UnknownKey("Ç".into()).message(&s(&["Ctrl", "Ç"])),
                   "Tecla não suportada. Escolha outra.");
    }
}
```

- [ ] **Step 2:** Run: `cargo test -p overlay-engine hotkey` → Expected: FAIL (não compila: `parse` não existe).
- [ ] **Step 3:** Implementar as funções do bloco Interfaces em `hotkey.rs`, com uma tabela estática nome → VK.
- [ ] **Step 4:** Run: `cargo test -p overlay-engine hotkey` → Expected: PASS (6 tests).
- [ ] **Step 5:** `git commit -am "feat(engine): add hotkey combo parsing and known conflicts"` (inclua `git add` dos arquivos novos)

---

### Task 3: Configurações e modelo de dados compartilhado

**Files:**
- Create: `crates/engine/src/settings.rs`, `crates/engine/src/model.rs`
- Modify: `crates/engine/Cargo.toml` (`serde` com derive, `serde_json`; dev: `tempfile`), `crates/engine/src/lib.rs`

**Interfaces:**
- Consumes: `hotkey::parse` (Task 2).
- Produces (`settings.rs`, todos com `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]`, `#[serde(rename_all = "camelCase")]` e `#[serde(default)]` nas structs; os enums também derivam `Copy`):
  - `pub struct Settings { pub version: u32, pub metrics: Metrics, pub style: Style, pub pos: Pos, pub size: u32, pub opacity: u32, pub shortcut: Vec<String>, pub general: General }`
  - `pub struct Metrics { pub fps: bool, pub graph: bool, pub gpu_use: bool, pub gpu_temp: bool, pub cpu_use: bool, pub cpu_temp: bool }` (Default = padrão da spec)
  - `pub enum Style { Line, Card }` e `pub enum Pos { Tl, Tc, Tr, Ml, Mr, Bl, Bc, Br }`, serializados em minúsculas
  - `pub struct General { pub startup: bool, pub minimized: bool, pub auto_show: bool }`
  - `impl Settings { pub fn normalized(self) -> Settings }`: `size` em 75..=200 e `opacity` em 30..=100, ambos arredondados ao múltiplo de 5 mais próximo; atalho inválido volta ao padrão.
  - `#[derive(Debug, PartialEq)] pub enum LoadOutcome { Loaded, Missing, Corrupt }`
  - `pub fn settings_path() -> PathBuf`: `%APPDATA%\perf-overlay\settings.json`
  - `pub fn load(path: &Path) -> (Settings, LoadOutcome)`
  - `pub fn save(path: &Path, s: &Settings) -> std::io::Result<()>`: cria a pasta; grava num `.tmp` e renomeia.
  - `pub fn apply_patch(current: &Settings, patch: serde_json::Value) -> Result<Settings, String>`: faz merge profundo do JSON, desserializa e normaliza.
- Produces (`model.rs`, camelCase):
  - `pub struct MetricsSample { pub fps: Option<u32>, pub frametime_ms: Option<f32>, pub graph: Vec<f32>, pub graph_max_ms: f32, pub gpu_use: Option<u8>, pub gpu_temp: Option<u8>, pub cpu_use: Option<u8>, pub cpu_temp: Option<u8> }` (Default)
  - `#[serde(tag = "state", rename_all = "camelCase")] pub enum Avail { Ok, NoSensor { reason: String } }`
  - `pub struct Availability { pub fps: Avail, pub gpu_use: Avail, pub gpu_temp: Avail, pub cpu_use: Avail, pub cpu_temp: Avail }` com `pub fn all_ok() -> Self`
  - `#[serde(rename_all = "lowercase")] pub enum DriverState { Missing, Installing, Failed, Installed }`
  - `pub struct Status { pub overlay_visible: bool, pub game_name: Option<String> }`
  - `pub struct AppState { pub settings: Settings, pub status: Status, pub availability: Availability, pub driver: DriverState, pub hotkey_error: Option<String> }`
  - `pub struct HotkeyResult { pub ok: bool, pub error: Option<String> }`
  - Motivos (`reason`) em pt-BR, como constantes: `REASON_NEEDS_ADMIN = "Execute o app como administrador."`, `REASON_NO_GPU_TEMP = "Esta GPU não informa a temperatura."`, `REASON_NO_GPU_COUNTERS = "Contadores de GPU indisponíveis."`, `REASON_CPU_UNSUPPORTED = "Este processador não é suportado."`

- [ ] **Step 1: Escrever os testes** (em `settings.rs` e `model.rs`)

```rust
// settings.rs
#[test]
fn defaults_match_spec() {
    assert_eq!(serde_json::to_value(Settings::default()).unwrap(), serde_json::json!({
        "version": 1,
        "metrics": {"fps": true, "graph": true, "gpuUse": true, "gpuTemp": true, "cpuUse": true, "cpuTemp": false},
        "style": "card", "pos": "tr", "size": 100, "opacity": 80,
        "shortcut": ["Ctrl", "Shift", "O"],
        "general": {"startup": true, "minimized": true, "autoShow": true}
    }));
}
#[test]
fn normalize_clamps_and_snaps_to_step_5() {
    let s = Settings { size: 203, opacity: 12, ..Settings::default() }.normalized();
    assert_eq!((s.size, s.opacity), (200, 30));
    let s = Settings { size: 77, opacity: 83, ..Settings::default() }.normalized();
    assert_eq!((s.size, s.opacity), (75, 85));
}
#[test]
fn invalid_shortcut_falls_back_to_default() {
    let s = Settings { shortcut: vec!["O".into()], ..Settings::default() }.normalized();
    assert_eq!(s.shortcut, vec!["Ctrl", "Shift", "O"]);
}
#[test]
fn load_missing_corrupt_and_partial() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.json");
    assert_eq!(load(&p), (Settings::default(), LoadOutcome::Missing));
    std::fs::write(&p, "{not json").unwrap();
    assert_eq!(load(&p), (Settings::default(), LoadOutcome::Corrupt));
    std::fs::write(&p, r#"{"style":"line","size":999,"opacity":0}"#).unwrap();
    let (s, o) = load(&p);
    assert_eq!(o, LoadOutcome::Loaded);
    assert_eq!((s.style, s.size, s.opacity), (Style::Line, 200, 30));
    assert_eq!(s.metrics, Metrics::default());
}
#[test]
fn save_creates_dir_and_roundtrips() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("perf-overlay").join("settings.json");
    let s = Settings { pos: Pos::Bl, ..Settings::default() };
    save(&p, &s).unwrap();
    assert_eq!(load(&p), (s, LoadOutcome::Loaded));
}
#[test]
fn apply_patch_merges_nested_and_rejects_bad_enum() {
    let s = apply_patch(&Settings::default(), serde_json::json!({"metrics": {"cpuTemp": true}, "pos": "bl"})).unwrap();
    assert!(s.metrics.cpu_temp && s.metrics.fps);
    assert_eq!(s.pos, Pos::Bl);
    assert!(apply_patch(&Settings::default(), serde_json::json!({"pos": "center"})).is_err());
}

// model.rs
#[test]
fn app_state_serializes_for_the_ui() {
    let st = AppState {
        settings: Settings::default(),
        status: Status { overlay_visible: true, game_name: Some("The Witcher 3".into()) },
        availability: Availability { gpu_temp: Avail::NoSensor { reason: "x".into() }, ..Availability::all_ok() },
        driver: DriverState::Missing,
        hotkey_error: None,
    };
    let v = serde_json::to_value(&st).unwrap();
    assert_eq!(v["status"]["overlayVisible"], true);
    assert_eq!(v["status"]["gameName"], "The Witcher 3");
    assert_eq!(v["availability"]["gpuTemp"], serde_json::json!({"state": "noSensor", "reason": "x"}));
    assert_eq!(v["availability"]["cpuUse"], serde_json::json!({"state": "ok"}));
    assert_eq!(v["driver"], "missing");
    assert!(v["hotkeyError"].is_null());
}
#[test]
fn metrics_sample_serializes_camel_case() {
    let v = serde_json::to_value(MetricsSample { frametime_ms: Some(7.5), graph_max_ms: 8.0, ..Default::default() }).unwrap();
    assert_eq!(v["frametimeMs"], 7.5);
    assert_eq!(v["graphMaxMs"], 8.0);
    assert!(v["fps"].is_null());
}
```

- [ ] **Step 2:** Run: `cargo test -p overlay-engine settings model` → Expected: FAIL (não compila).
- [ ] **Step 3:** Implementar os tipos e as funções do bloco Interfaces.
- [ ] **Step 4:** Run: `cargo test -p overlay-engine` → Expected: PASS.
- [ ] **Step 5:** Commit: `feat(engine): add settings persistence and shared ui model`

---

### Task 4: Thread nativa, timers e registro do atalho global

**Files:**
- Create: `crates/engine/src/native/mod.rs`, `crates/engine/examples/hold_hotkey.rs`
- Modify: `crates/engine/Cargo.toml` (dependência `windows` com as features de mensagens, teclado e threads), `crates/engine/src/lib.rs`

**Interfaces:**
- Consumes: `hotkey::{Hotkey, HotkeyError}`.
- Produces:
  - `#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum TimerKind { Sample, Graph, Detect }`: 500 ms, 66 ms e 250 ms.
  - `#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)] pub struct Timers { pub sample: bool, pub graph: bool, pub detect: bool }`
  - `pub enum NativeEvent { Hotkey, Timer(TimerKind), DisplayChanged }`. A Task 12 acrescenta `Foreground(ForegroundInfo)`.
  - `pub enum NativeCmd { RegisterHotkey(Hotkey, std::sync::mpsc::Sender<Result<(), HotkeyError>>), UnregisterHotkey, SetTimers(Timers), Quit }`. A Task 11 acrescenta `Overlay(OverlayCmd)`.
  - `pub struct NativeHandle`, com:
    - `pub fn spawn(on_event: Box<dyn Fn(NativeEvent) + Send + 'static>) -> NativeHandle`
    - `pub fn send(&self, cmd: NativeCmd)`
    - `pub fn register_hotkey(&self, hk: Hotkey) -> Result<(), HotkeyError>`: bloqueia até a resposta; falha do `RegisterHotKey` vira `InUse`.
    - `pub fn quit(self)`: manda `Quit` e faz join.

Desenho: uma thread com janela message-only (`HWND_MESSAGE`) e loop `GetMessageW`. Os comandos chegam por `mpsc::channel` e a thread é acordada com `PostMessageW(hwnd, WM_APP + 1)`. Os timers usam `SetTimer`/`KillTimer` na janela com IDs fixos e só existem quando o `Timers` correspondente está ligado, sem timer ocioso. `WM_HOTKEY` gera `NativeEvent::Hotkey`. `on_event` roda na thread nativa.

- [ ] **Step 1: Escrever os testes** (em `native/mod.rs`)

```rust
#[test]
fn second_registration_of_same_combo_reports_in_use() {
    let a = NativeHandle::spawn(Box::new(|_| {}));
    let b = NativeHandle::spawn(Box::new(|_| {}));
    let combo: Vec<String> = ["Ctrl", "Alt", "Shift", "F11"].iter().map(|x| x.to_string()).collect();
    let hk = crate::hotkey::parse(&combo).unwrap();
    assert_eq!(a.register_hotkey(hk), Ok(()));
    assert_eq!(b.register_hotkey(hk), Err(HotkeyError::InUse));
    a.quit();
    b.quit();
}
#[test]
fn timers_fire_only_when_enabled() {
    use std::{sync::mpsc, time::Duration};
    let (tx, rx) = mpsc::channel();
    let h = NativeHandle::spawn(Box::new(move |e| if let NativeEvent::Timer(k) = e { let _ = tx.send(k); }));
    assert!(rx.recv_timeout(Duration::from_millis(700)).is_err());
    h.send(NativeCmd::SetTimers(Timers { sample: true, ..Timers::default() }));
    assert_eq!(rx.recv_timeout(Duration::from_millis(1200)), Ok(TimerKind::Sample));
    h.send(NativeCmd::SetTimers(Timers::default()));
    std::thread::sleep(Duration::from_millis(100));
    while rx.try_recv().is_ok() {}
    assert!(rx.recv_timeout(Duration::from_millis(700)).is_err());
    h.quit();
}
```

- [ ] **Step 2:** Run: `cargo test -p overlay-engine native` → Expected: FAIL (não compila).
- [ ] **Step 3:** Implementar `NativeHandle` conforme o desenho acima.
- [ ] **Step 4:** Run: `cargo test -p overlay-engine native` → Expected: PASS (2 tests).
- [ ] **Step 5:** `examples/hold_hotkey.rs`: recebe `Ctrl+Alt+Shift+F9 --seconds N`, registra o atalho com `NativeHandle`, imprime `held` e espera N segundos. Será usado na Task 15 pra simular "atalho em uso por outro app". Run: `cargo run -p overlay-engine --example hold_hotkey -- Ctrl+Alt+Shift+F9 --seconds 2` → Expected: imprime `held` e sai.
- [ ] **Step 6:** Commit: `feat(engine): add native message thread with timers and global hotkey`

---

### Task 5: Estatística de quadros

**Files:**
- Create: `crates/engine/src/frames/mod.rs`, `crates/engine/src/frames/stats.rs`, `crates/engine/src/frames/clock.rs`

**Interfaces:**
- Produces:
  - `clock.rs`: `pub fn now_ns() -> u64` = `GetSystemTimePreciseAsFileTime` × 100 (o mesmo relógio dos timestamps ETW, que chegam em FILETIME).
  - `stats.rs`: `pub const WINDOW_NS: u64 = 500_000_000; pub const HISTORY_NS: u64 = 3_000_000_000; pub const STALE_NS: u64 = 1_000_000_000; pub const STREAK_GAP_NS: u64 = 250_000_000;`
  - `pub struct FrameTracker`, com:
    - `new()`, `on_present(&mut self, t_ns: u64)`, `clear(&mut self)`, `len(&self) -> usize`
    - `fps(&self, now_ns: u64) -> Option<u32>`
    - `avg_frametime_ms(&self, now_ns: u64) -> Option<f32>`
    - `graph(&self, now_ns: u64, columns: usize) -> Vec<f32>`
  - `pub fn graph_y_max(columns: &[f32]) -> f32`
  - `pub fn downsample_max(values: &[f32], n: usize) -> Vec<f32>`
  - `pub struct PresentObserver`, com `new()`, `on_present(&mut self, pid: u32, t_ns: u64)`, `continuous_ms(&self, pid: u32, now_ns: u64) -> u64`, `clear(&mut self)`.

Regras:
- **Histórico:** `on_present` descarta presents mais velhos que `HISTORY_NS`.
- **FPS e frametime:** com os presents em `(now − WINDOW_NS, now]`, `fps = round((n−1)·1e9 / (último − primeiro))` e `avg_frametime_ms = (último − primeiro) / (n−1) / 1e6`. Exige `n ≥ 2`. Se `now − último > STALE_NS`, os dois dão `None`.
- **`graph`:** divide `[now − HISTORY_NS, now]` em `columns` colunas iguais. Cada frametime (intervalo até o present anterior) conta na coluna do present que o fecha, e a coluna guarda o **máximo**. Coluna vazia repete a anterior. Colunas vazias no começo copiam a primeira coluna com dado. Sem nenhum dado, devolve vetor vazio.
- **`graph_y_max`:** `v = max(colunas) × 1.15`; arredonda pra cima no passo 2 se `v < 20`, passo 5 se `v < 50`, passo 10 se `v < 200`, senão passo 50. Mínimo 8.
- **`downsample_max`:** agrupa em `n` grupos contíguos e pega o máximo de cada um. Se `n ≥ len`, devolve a entrada sem mudança.
- **`PresentObserver`:** guarda, por pid, (início da sequência, último present). Um intervalo maior que `STREAK_GAP_NS` reinicia a sequência. `continuous_ms` dá 0 se `now − último > STREAK_GAP_NS`.

- [ ] **Step 1: Escrever os testes** (em `stats.rs`)

```rust
fn feed(t: &mut FrameTracker, fps: f64, secs: f64, start: u64) -> u64 {
    let dt = (1e9 / fps) as u64;
    let n = (fps * secs) as u64;
    for i in 0..n { t.on_present(start + i * dt); }
    start + (n - 1) * dt
}
#[test]
fn fps_and_frametime_for_steady_rates() {
    for (fps, ft) in [(142.0, 7.04), (60.0, 16.67), (30.0, 33.33)] {
        let mut t = FrameTracker::new();
        let now = feed(&mut t, fps, 2.0, 1_000_000_000);
        assert_eq!(t.fps(now), Some(fps as u32));
        assert!((t.avg_frametime_ms(now).unwrap() - ft).abs() < 0.01);
    }
}
#[test]
fn stale_after_one_second_without_presents() {
    let mut t = FrameTracker::new();
    let now = feed(&mut t, 60.0, 1.0, 0);
    assert!(t.fps(now + STALE_NS + 1).is_none());
    assert!(t.avg_frametime_ms(now + STALE_NS + 1).is_none());
}
#[test]
fn history_is_bounded_at_3000_fps() {
    let mut t = FrameTracker::new();
    feed(&mut t, 3000.0, 10.0, 0);
    assert!(t.len() <= 9001);
}
#[test]
fn graph_keeps_a_50ms_spike_in_one_column() {
    let mut t = FrameTracker::new();
    let mid = feed(&mut t, 60.0, 1.5, 0);
    let now = feed(&mut t, 60.0, 1.5, mid + 50_000_000);
    let g = t.graph(now, 126);
    assert_eq!(g.len(), 126);
    assert!(g.iter().cloned().fold(0.0, f32::max) >= 49.9);
    assert!(g.iter().filter(|&&v| v > 20.0).count() <= 2);
    assert!(FrameTracker::new().graph(0, 44).is_empty());
}
#[test]
fn y_max_nice_ceiling() {
    for (cols, want) in [(vec![], 8.0), (vec![6.9], 8.0), (vec![7.9], 10.0), (vec![16.7], 20.0),
                         (vec![40.0], 50.0), (vec![100.0], 120.0), (vec![500.0], 600.0)] {
        assert_eq!(graph_y_max(&cols), want, "{cols:?}");
    }
}
#[test]
fn downsample_keeps_max() {
    assert_eq!(downsample_max(&[1.0, 1.0, 9.0, 1.0, 1.0, 1.0], 3), vec![1.0, 9.0, 1.0]);
}
#[test]
fn observer_streak_resets_after_gap_and_quiet() {
    let mut o = PresentObserver::new();
    for i in 0..=150u64 { o.on_present(7, i * 16_666_666); }
    let now = 150 * 16_666_666;
    assert!(o.continuous_ms(7, now) >= 2400);
    assert_eq!(o.continuous_ms(9, now), 0);
    assert_eq!(o.continuous_ms(7, now + 300_000_000), 0);
    o.on_present(7, now + 300_000_000);
    assert_eq!(o.continuous_ms(7, now + 300_000_000), 0);
}
```

- [ ] **Step 2:** Run: `cargo test -p overlay-engine frames` → Expected: FAIL (não compila).
- [ ] **Step 3:** Implementar conforme as regras acima.
- [ ] **Step 4:** Run: `cargo test -p overlay-engine frames` → Expected: PASS (7 tests).
- [ ] **Step 5:** Commit: `feat(engine): add frame tracker, graph bucketing and present observer`

---

### Task 6: Captura ETW, jogo falso e sonda de quadros

**Files:**
- Create: `crates/engine/src/frames/etw.rs`, `crates/engine/examples/fake_game.rs`, `crates/engine/examples/frames_probe.rs`, `scripts/run-elevated.ps1`
- Modify: `crates/engine/Cargo.toml` (`ferrisetw = "1"`; features `windows` de ETW; em dev-dependencies, as features D3D11/DXGI que só o `fake_game` usa)

**Interfaces:**
- Consumes: `FrameTracker`, `PresentObserver`, `now_ns` (Task 5).
- Produces:
  - `pub const SESSION_NAME: &str = "PerfOverlay-Frames";`
  - `pub struct FrameShared { pub tracker: Mutex<FrameTracker>, pub observer: Mutex<PresentObserver>, /* target: AtomicU32, 0 = nenhum */ }`, com:
    - `pub fn new() -> Arc<Self>`
    - `pub fn set_target(&self, pid: Option<u32>)`: limpa o tracker quando o alvo muda.
    - `pub fn target(&self) -> Option<u32>`
    - `pub fn on_present(&self, pid: u32, t_ns: u64)`: sempre alimenta o observer; o tracker, só se `pid` for o alvo.
  - `#[derive(Debug)] pub enum CaptureError { AccessDenied, Other(String) }`
  - `pub struct FrameCapture`, com `pub fn start(shared: Arc<FrameShared>) -> Result<FrameCapture, CaptureError>` e `pub fn stop(self)`.

Desenho:
- **Sessão:** `ferrisetw::UserTrace` nomeada `SESSION_NAME`, com os providers Microsoft-Windows-DXGI `{CA11C036-0102-4A2D-A6AD-F03CFED5D3C9}` (event IDs `Present_Start` = 42 e `PresentMultiplaneOverlay_Start` = 55) e Microsoft-Windows-D3D9 `{783ACA0A-790E-4D7F-8451-AA850511C6B9}` (`Present_Start` = 1). Confira IDs, nível e keywords em `PresentData/ETW/Microsoft_Windows_DXGI.h`, `Microsoft_Windows_D3D9.h` e `PresentMonTraceSession.cpp` do repositório GameTechDev/PresentMon.
- **Callback:** `shared.on_present(record.process_id(), record.raw_timestamp() as u64 * 100)` e nada mais, em O(1).
- **Sem admin:** `ERROR_ACCESS_DENIED` vira `AccessDenied`.
- **Sessão órfã:** se o start falhar com `ERROR_ALREADY_EXISTS`, pare a sessão antiga (`ControlTraceW(.., SESSION_NAME, .., EVENT_TRACE_CONTROL_STOP)`), chame `trace!("stopped stale session")` e tente de novo uma vez.

- [ ] **Step 1: Escrever o teste** (em `etw.rs`)

```rust
#[test]
fn shared_routes_only_target_presents_to_tracker() {
    let s = FrameShared::new();
    s.set_target(Some(2));
    for i in 0..10u64 { s.on_present(1, i * 16_666_666); s.on_present(2, i * 16_666_666); }
    assert_eq!(s.tracker.lock().unwrap().len(), 10);
    assert!(s.observer.lock().unwrap().continuous_ms(1, 9 * 16_666_666) > 0);
    s.set_target(Some(1));
    assert_eq!(s.tracker.lock().unwrap().len(), 0);
}
```

- [ ] **Step 2:** Run: `cargo test -p overlay-engine etw` → Expected: FAIL (não compila).
- [ ] **Step 3:** Implementar `FrameShared` e `FrameCapture` conforme o desenho.
- [ ] **Step 4:** Run: `cargo test -p overlay-engine etw` → Expected: PASS.
- [ ] **Step 5: `fake_game`.**
  - Flags: `--windowed` (800×600; sem ela, popup sem borda cobrindo o monitor primário), `--fps N` (0 = vsync com `Present(1,0)`; senão `Present(0,0)` + espera até o próximo quadro), `--seconds N`.
  - Janela com título "Fake Game", que chama `SetForegroundWindow` ao ser criada, e swap chain flip-model (`CreateSwapChainForHwnd`, `DXGI_SWAP_EFFECT_FLIP_DISCARD`, 2 buffers). Cada quadro limpa a tela com uma cor que varia.
  - Imprime `pid=<n>` ao iniciar.
- [ ] **Step 6: `frames_probe`.**
  - Flags: `--pid N --seconds N` e `--crash-after N` (sai com `std::process::exit(1)` sem parar a sessão).
  - No primeiro evento imprime `clock_skew_ms=<now_ns − t_evento em ms>`.
  - A cada 500 ms imprime `fps=<..> ft=<..>`.
  - Sem permissão, imprime `error=AccessDenied` e sai com código 0.
- [ ] **Step 7: `scripts/run-elevated.ps1`.**
  - Parâmetros `-Exe`, `-ArgList` (string), `-Out`.
  - Executa `Start-Process cmd -Verb RunAs -Wait -ArgumentList "/c `"$Exe`" $ArgList > `"$Out`" 2>&1"` e depois `Get-Content $Out`.
- [ ] **Step 8 [ADMIN]: Validar a captura.**
  - Rode `cargo build -p overlay-engine --examples`.
  - Inicie `fake_game --windowed --fps 60 --seconds 25` com `Start-Process` e anote o pid.
  - Rode `scripts\run-elevated.ps1 -Exe target\debug\examples\frames_probe.exe -ArgList "--pid <pid> --seconds 8" -Out $env:TEMP\probe.txt`.
  - Expected: `|clock_skew_ms| < 1000` e a maioria das linhas com `fps` entre 57 e 63 e `ft` entre 15,9 e 17,5.
- [ ] **Step 9 [ADMIN]: Sessão órfã** (Review Focus 3).
  - Rode o probe elevado com `--crash-after 2` e, logo depois, de novo normalmente.
  - Expected: a segunda execução imprime `stopped stale session` e linhas `fps=`.
- [ ] **Step 10:** Rode `target\debug\examples\frames_probe.exe --pid 4 --seconds 1` **sem** elevação. Expected: `error=AccessDenied`.
- [ ] **Step 11:** Commit: `feat(engine): capture dxgi/d3d9 presents via etw with fake game fixture`

---

### Task 7: Sensores de uso de CPU/GPU e temperatura da GPU

**Files:**
- Create: `crates/engine/src/sensors/{mod,cpu_usage,gpu_usage,gpu_temp,adapter}.rs`, `crates/engine/examples/sensors_probe.rs`
- Modify: `crates/engine/Cargo.toml` (features `windows`: Performance, Dxgi e `Wdk_Graphics_Direct3D` pra D3DKMT; se o D3DKMT faltar no crate, declare via `#[link(name = "gdi32")]`)

**Interfaces:**
- Consumes: `model::{Avail, REASON_*}`, `settings::Metrics`.
- Produces:
  - `pub type Luid = u64;` e `pub fn luid_from_parts(high: i32, low: u32) -> Luid`: `(high as u32 as u64) << 32 | low`.
  - `#[derive(Clone, Debug, PartialEq)] pub struct EngineInstance { pub pid: u32, pub luid: Luid, pub engtype: String }`
  - `pub fn parse_engine_instance(name: &str) -> Option<EngineInstance>`
  - `pub fn pick_adapter_usage(samples: &[(EngineInstance, f64)], game_pid: Option<u32>, default_luid: Option<Luid>) -> Option<(Luid, u8)>`: soma `engtype == "3D"` de todos os processos por luid, com limite de 100. A ordem de escolha do luid é: o do jogo (a instância 3D do pid com maior uso), senão `default_luid`, senão o de maior soma.
  - `pub struct CpuUsage`: `new() -> Result<Self, String>` e `sample(&mut self) -> Option<u8>`. Usa `\Processor Information(_Total)\% Processor Utility` via `PdhAddEnglishCounterW`, com limite de 100.
  - `pub struct GpuUsage`: `new() -> Result<Self, String>` e `sample(&mut self, game_pid: Option<u32>, default_luid: Option<Luid>) -> Option<(Luid, u8)>`. Usa `\GPU Engine(*)\Utilization Percentage` via `PdhAddEnglishCounterW` e `PdhGetFormattedCounterArrayW`.
  - `pub fn deci_celsius_to_c(v: u32) -> Option<u8>`: 0 vira `None`; arredonda pra cima a partir de 0,5.
  - `pub fn read_gpu_temp(luid: Luid) -> Option<u8>`: `D3DKMTOpenAdapterFromLuid` + `D3DKMTQueryAdapterInfo(KMTQAITYPE_ADAPTERPERFDATA)`, `PhysicalAdapterIndex = 0`, campo `Temperature`.
  - `pub fn default_dedicated_luid() -> Option<Luid>`: `CreateDXGIFactory1` + `EnumAdapters1`, maior `DedicatedVideoMemory`, ignorando `DXGI_ADAPTER_FLAG_SOFTWARE`. Não cria dispositivo.
  - `#[derive(Clone, Copy, Debug, Default, PartialEq)] pub struct SensorSample { pub cpu_use: Option<u8>, pub gpu_use: Option<u8>, pub gpu_temp: Option<u8>, pub cpu_temp: Option<u8> }`
  - `pub struct SensorAvailability { pub cpu_use: Avail, pub gpu_use: Avail, pub gpu_temp: Avail, pub cpu_temp: Avail }`
  - `pub struct Sensors`, com:
    - `pub fn new() -> Sensors`: abre o que conseguir. GPU temp fica `Ok` se `read_gpu_temp(default)` devolver valor, senão `NoSensor(REASON_NO_GPU_TEMP)`. Nesta task, `cpu_temp` é sempre `NoSensor(REASON_CPU_UNSUPPORTED)`.
    - `pub fn availability(&self) -> SensorAvailability`
    - `pub fn sample(&mut self, wanted: &Metrics, game_pid: Option<u32>) -> SensorSample`: só lê o que `wanted` pede.

- [ ] **Step 1: Escrever os testes**

```rust
// gpu_usage.rs
fn inst(pid: u32, luid: Luid, e: &str) -> EngineInstance { EngineInstance { pid, luid, engtype: e.into() } }
#[test]
fn parses_pdh_engine_instance_names() {
    let e = parse_engine_instance("pid_1234_luid_0x00000000_0x0000D1B7_phys_0_eng_0_engtype_3D").unwrap();
    assert_eq!(e, inst(1234, 0xD1B7, "3D"));
    let e = parse_engine_instance("pid_88_luid_0x00000001_0x0000ABCD_phys_0_eng_3_engtype_VideoDecode").unwrap();
    assert_eq!(e, inst(88, 0x1_0000_ABCD, "VideoDecode"));
    assert!(parse_engine_instance("_Total").is_none());
}
#[test]
fn picks_game_adapter_and_sums_3d_of_all_processes() {
    let (a, b) = (0xA, 0xB);
    let s = vec![(inst(100, a, "3D"), 60.0), (inst(200, a, "3D"), 30.0), (inst(100, a, "Copy"), 50.0), (inst(300, b, "3D"), 10.0)];
    assert_eq!(pick_adapter_usage(&s, Some(100), Some(b)), Some((a, 90)));
    assert_eq!(pick_adapter_usage(&s, None, Some(b)), Some((b, 10)));
    assert_eq!(pick_adapter_usage(&s, Some(999), None), Some((a, 90)));
    assert_eq!(pick_adapter_usage(&[(inst(1, a, "3D"), 80.0), (inst(2, a, "3D"), 45.0)], None, None), Some((a, 100)));
}
// gpu_temp.rs
#[test]
fn deci_celsius_conversion() {
    assert_eq!(deci_celsius_to_c(712), Some(71));
    assert_eq!(deci_celsius_to_c(715), Some(72));
    assert_eq!(deci_celsius_to_c(0), None);
}
```

- [ ] **Step 2:** Run: `cargo test -p overlay-engine sensors` → Expected: FAIL (não compila).
- [ ] **Step 3:** Implementar as funções puras, depois `CpuUsage`, `GpuUsage`, `read_gpu_temp`, `default_dedicated_luid` e `Sensors`.
- [ ] **Step 4:** Run: `cargo test -p overlay-engine sensors` → Expected: PASS (3 tests).
- [ ] **Step 5: `sensors_probe`.** Flag `--seconds N`; imprime a cada 500 ms `cpu=<..> gpu=<..> gpu_temp=<..> cpu_temp=<..>`, com `—` pra `None`.
- [ ] **Step 6: Comparar com o Gerenciador de Tarefas** (Review Focus 5). Esta máquina usa Windows em português.
  - Run: `cargo run -p overlay-engine --example sensors_probe -- --seconds 10` sem elevação, com o Gerenciador de Tarefas aberto na aba Desempenho.
  - Expected: `cpu` a ±5 pontos da CPU do Gerenciador; `gpu` a ±5 do gráfico "3D" da RTX 4070 Ti; `gpu_temp` a ±2 °C da temperatura mostrada lá. Nenhum erro de contador não encontrado.
- [ ] **Step 7:** Commit: `feat(engine): add cpu/gpu usage via pdh and gpu temperature via d3dkmt`

---

### Task 8: PawnIO, temperatura da CPU e instalação via winget

**Files:**
- Create: `crates/engine/src/pawnio/mod.rs`, `crates/engine/src/pawnio/install.rs`, `crates/engine/src/sensors/cpu_temp.rs`, `crates/engine/resources/pawnio/{AMDFamily17.bin,IntelMSR.bin,README.md}`
- Modify: `crates/engine/src/sensors/mod.rs`, `crates/engine/Cargo.toml` (`libloading = "0.8"`, feature `windows` de Registry)

**Interfaces:**
- Consumes: `Sensors`, `SensorAvailability` (Task 7); `model::{Avail, REASON_*}`.
- Produces:
  - `pawnio`:
    - `pub fn find_lib() -> Option<PathBuf>`: procura `InstallLocation` em `HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\PawnIO` e junta `PawnIOLib.dll`; se não achar, usa `%ProgramFiles%\PawnIO\PawnIOLib.dll`. Só retorna se o arquivo existir.
    - `pub fn is_installed() -> bool`
    - `#[derive(Debug)] pub enum PawnIoError { NotInstalled, AccessDenied, Failed(String) }`
    - `pub struct PawnIo`, com `open(lib: &Path) -> Result<Self, PawnIoError>`, `load_module(&mut self, blob: &[u8]) -> Result<(), PawnIoError>` e `execute(&self, func: &str, input: &[u64], out_len: usize) -> Result<Vec<u64>, PawnIoError>`. Usa as funções `pawnio_open`, `pawnio_load`, `pawnio_execute` e `pawnio_close` da DLL, via `libloading`.
    - `pub static AMD_FAMILY17_BIN: &[u8]` e `pub static INTEL_MSR_BIN: &[u8]`, via `include_bytes!`.
  - `pawnio::install`: `pub fn winget_args() -> [&'static str; 8]` e `pub fn run_install() -> Result<(), String>`. Roda `winget` sem janela (`CREATE_NO_WINDOW`), espera terminar e devolve `Ok` se `is_installed()` passar a ser verdadeiro, qualquer que seja o código de saída.
  - `cpu_temp`:
    - `#[derive(Debug, PartialEq)] pub enum CpuKind { AmdZen, Intel, Unsupported }`
    - `pub fn cpu_kind_from(vendor: &str, family: u32) -> CpuKind` e `pub fn cpu_kind() -> CpuKind`: CPUID leaf 0 pro vendor e leaf 1 pra família (base + estendida quando a base é 0xF).
    - `pub fn zen_tctl_c(smn_reg: u32) -> f32`: `((reg >> 21) & 0x7FF) × 0.125`, menos 49 se o bit 19 estiver ligado.
    - `pub fn intel_pkg_c(temperature_target: u64, package_therm_status: u64) -> f32`: `TjMax = (target >> 16) & 0xFF`, `readout = (status >> 16) & 0x7F`, resultado `TjMax − readout`.
    - `pub struct CpuTemp`, com `open() -> Result<Self, Avail>` e `read(&self) -> Option<u8>`. AMD Zen usa o módulo AMDFamily17, função de leitura SMN no endereço `0x0005_9800`. Intel usa o módulo IntelMSR, MSR `0x1A2` (temperature target) e `0x1B1` (package therm status). Confira os nomes exatos das funções e o layout de entrada e saída no código dos módulos (`.p`) do repositório namazso/PawnIO.Modules. O esperado é `ioctl_read_smn` e `ioctl_read_msr`.
  - `Sensors` passa a abrir `CpuTemp` em `new()`. `cpu_temp` fica `Ok`, ou `NoSensor(REASON_NEEDS_ADMIN)` se for `AccessDenied`, ou `NoSensor(REASON_CPU_UNSUPPORTED)` se for `Unsupported`. Sem PawnIO instalado, `cpu_temp` fica `Ok`, e é o `DriverState` que bloqueia. Acrescente `pub fn reload_cpu_temp(&mut self)`.

- [ ] **Step 1: Baixar os módulos.**
  - Baixe a release mais recente de namazso/PawnIO.Modules numa pasta nova e vazia em `%TEMP%`.
  - Copie só `AMDFamily17.bin` e `IntelMSR.bin` pra `crates/engine/resources/pawnio/`.
  - Escreva em `README.md` a versão, a URL e a licença.
  - Confirme também o id do winget com `winget show --id namazso.PawnIO -e` (Expected: mostra o pacote PawnIO).
- [ ] **Step 2: Escrever os testes**

```rust
// cpu_temp.rs
#[test]
fn classifies_cpus() {
    assert_eq!(cpu_kind_from("AuthenticAMD", 0x19), CpuKind::AmdZen);
    assert_eq!(cpu_kind_from("AuthenticAMD", 0x17), CpuKind::AmdZen);
    assert_eq!(cpu_kind_from("AuthenticAMD", 0x15), CpuKind::Unsupported);
    assert_eq!(cpu_kind_from("GenuineIntel", 6), CpuKind::Intel);
    assert_eq!(cpu_kind_from("HygonGenuine", 0x18), CpuKind::Unsupported);
}
#[test]
fn zen_tctl_conversion() {
    assert_eq!(zen_tctl_c(512 << 21), 64.0);
    assert_eq!(zen_tctl_c((512 << 21) | (1 << 19)), 15.0);
    assert_eq!(zen_tctl_c(0x2C0 << 21), 88.0);
}
#[test]
fn intel_package_conversion() {
    assert_eq!(intel_pkg_c(100 << 16, 36 << 16), 64.0);
    assert_eq!(intel_pkg_c(0x5F << 16, 0x1E << 16), 65.0);
}
// install.rs
#[test]
fn winget_command_is_silent_and_exact() {
    assert_eq!(winget_args(), ["install", "--id", "namazso.PawnIO", "-e", "--silent",
        "--accept-package-agreements", "--accept-source-agreements", "--disable-interactivity"]);
}
// pawnio/mod.rs
#[test]
fn embedded_modules_are_present() {
    assert!(AMD_FAMILY17_BIN.len() > 64 && INTEL_MSR_BIN.len() > 64);
}
```

- [ ] **Step 3:** Run: `cargo test -p overlay-engine cpu_temp install pawnio` → Expected: FAIL (não compila).
- [ ] **Step 4:** Implementar conforme o bloco Interfaces.
- [ ] **Step 5:** Run: `cargo test -p overlay-engine` → Expected: PASS.
- [ ] **Step 6 [ADMIN]: Validar com o driver.**
  - **Pergunte ao usuário** se pode instalar o PawnIO agora. Se não puder, registre "não validado" no relatório da task e siga.
  - Se puder: `scripts\run-elevated.ps1 -Exe winget -ArgList "<winget_args>" -Out $env:TEMP\winget.txt`, depois o `sensors_probe` elevado.
  - Expected: `cpu_temp` entre 30 e 90 no Ryzen 7 5800X, subindo sob carga (por exemplo, com o `fake_game --fps 0` rodando).
- [ ] **Step 7:** Commit: `feat(engine): read cpu temperature through pawnio and install via winget`

---

### Task 9: Detecção de jogo, informações de processo e efeitos

**Files:**
- Create: `crates/engine/src/game.rs`, `crates/engine/src/process_info.rs`, `crates/engine/src/effects.rs`

**Interfaces:**
- Consumes: `settings::Metrics`, `model::{Availability, Avail, DriverState}`.
- Produces:
  - `game.rs`:
    - `pub const CONFIRM_MS: u64 = 2000;`
    - `#[derive(Clone, Copy, Debug, PartialEq)] pub enum DetectPath { Fullscreen, Windowed }`
    - `#[derive(Clone, Debug, PartialEq)] pub struct Fg { pub pid: u32, pub hwnd: isize, pub covers_monitor: bool, pub ignored: bool }`
    - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct Game { pub pid: u32, pub hwnd: isize, pub path: DetectPath }`
    - `pub struct GameDetector`, com:
      - `new(auto_show: bool)` e `set_auto_show(&mut self, on: bool)`: reavalia o foreground atual.
      - Entradas: `on_foreground(&mut self, fg: Fg)`, `on_presenting(&mut self, pid: u32, continuous_ms: u64)`, `on_process_exited(&mut self, pid: u32)`, `toggle(&mut self)`, `set_visible(&mut self, v: bool)`.
      - Leituras: `visible(&self) -> bool`, `game(&self) -> Option<Game>`, `candidate_pid(&self) -> Option<u32>`, `anchor_hwnd(&self) -> Option<isize>`.
  - `process_info.rs`:
    - `pub fn is_ignored(exe_name: &str, self_exe: &str) -> bool`
    - `pub fn display_name(file_description: Option<&str>, window_title: Option<&str>, exe_name: Option<&str>) -> String`
    - `pub fn exe_path(pid: u32) -> Option<PathBuf>`: `QueryFullProcessImageNameW` com `PROCESS_QUERY_LIMITED_INFORMATION`.
    - `pub fn file_description(path: &Path) -> Option<String>`: `GetFileVersionInfoW` + `VerQueryValueW` pela primeira entrada de `\VarFileInfo\Translation`.
  - `effects.rs`:
    - `#[derive(Clone, Copy, Debug, Default, PartialEq)] pub struct Effects { pub etw: bool, pub sample_timer: bool, pub graph_timer: bool, pub detect_timer: bool }`
    - `pub fn plan_effects(visible: bool, config_open: bool, candidate: bool, m: &Metrics) -> Effects`
    - `pub fn effective_metrics(m: &Metrics, a: &Availability, driver: DriverState) -> Metrics`

Regras (spec 4.6):
- **`on_foreground`:** guarda o fg.
  - Se o fg é o jogo detectado: atualiza o hwnd. Se o overlay está oculto, `auto_show` está ligado, o caminho é `Fullscreen` e não há supressão pra esse pid, mostra como "auto".
  - Senão, se havia overlay "auto" visível: esconde.
  - Candidato: `ignored` nunca é candidato; `covers_monitor && (auto_show || visible)` é candidato `Fullscreen`; com `visible` (e sem tela cheia), candidato `Windowed`; fora isso, sem candidato.
- **`on_presenting(pid, ms)`:** com `ms ≥ CONFIRM_MS` e pid igual ao candidato:
  - O candidato vira o jogo. Se for outro pid, limpa a supressão.
  - Se o caminho é `Fullscreen`, `auto_show` está ligado e o overlay está oculto, mostra como "auto".
- **`on_process_exited(pid)`:** se for o jogo, zera jogo e supressão. Overlay "auto" se esconde; "manual" continua visível.
- **`toggle` / `set_visible(false)`:**
  - Esconder: se há jogo, suprime aquele pid.
  - Mostrar: modo "manual". Se o fg atual não é ignorado nem é o jogo, vira candidato `Windowed`.
- **`anchor_hwnd`:** hwnd do jogo; senão, hwnd do fg.
- **`plan_effects`:** `sample_timer = visible || config_open`; `graph_timer = visible && m.graph`; `etw = candidate || ((visible || config_open) && (m.fps || m.graph))`; `detect_timer = candidate`.
- **`effective_metrics`:** desliga a métrica cuja `Avail` não é `Ok`. Desliga `cpu_temp` se `driver != Installed`. `graph` segue a disponibilidade de `fps`.
- **`is_ignored`:** sem diferenciar maiúsculas: `explorer.exe`, `dwm.exe`, `shellexperiencehost.exe`, `searchhost.exe`, `startmenuexperiencehost.exe`, `applicationframehost.exe`, `textinputhost.exe`, `lockapp.exe`, `msedge.exe`, `chrome.exe`, `firefox.exe`, `opera.exe`, `brave.exe`, `vivaldi.exe`, `msedgewebview2.exe`, e `self_exe`.
- **`display_name`:** primeiro valor não vazio, depois de `trim`: descrição, título, nome do exe sem `.exe`; se nada sobrar, "Jogo".

- [ ] **Step 1: Escrever os testes**

```rust
// game.rs
fn fg(pid: u32, full: bool) -> Fg { Fg { pid, hwnd: pid as isize * 10, covers_monitor: full, ignored: false } }
fn gpid(d: &GameDetector) -> Option<u32> { d.game().map(|g| g.pid) }

#[test]
fn fullscreen_game_confirmed_after_2s_auto_shows() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true));
    assert_eq!(d.candidate_pid(), Some(1));
    d.on_presenting(1, 1999);
    assert!(d.game().is_none());
    d.on_presenting(1, CONFIRM_MS);
    assert_eq!(gpid(&d), Some(1));
    assert!(d.visible());
    assert_eq!(d.candidate_pid(), None);
}
#[test]
fn ignored_process_is_never_candidate() {
    let mut d = GameDetector::new(true);
    d.on_foreground(Fg { ignored: true, ..fg(2, true) });
    assert_eq!(d.candidate_pid(), None);
}
#[test]
fn windowed_process_is_candidate_only_while_visible() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(3, false));
    assert_eq!(d.candidate_pid(), None);
    d.toggle();
    assert_eq!(d.candidate_pid(), Some(3));
    d.on_presenting(3, CONFIRM_MS);
    assert_eq!(d.game().unwrap().path, DetectPath::Windowed);
    assert!(d.visible());
}
#[test]
fn auto_overlay_hides_on_focus_loss_and_returns() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS);
    d.on_foreground(fg(9, false));
    assert!(!d.visible());
    assert_eq!(gpid(&d), Some(1));
    d.on_foreground(fg(1, true));
    assert!(d.visible());
}
#[test]
fn hotkey_hide_suppresses_until_game_exits() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS);
    d.toggle();
    assert!(!d.visible());
    d.on_foreground(fg(9, false)); d.on_foreground(fg(1, true));
    assert!(!d.visible(), "Alt+Tab não limpa a supressão");
    d.on_process_exited(1);
    assert!(d.game().is_none());
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS);
    assert!(d.visible());
}
#[test]
fn another_game_clears_suppression() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS); d.toggle();
    d.on_foreground(fg(2, true)); d.on_presenting(2, CONFIRM_MS);
    assert!(d.visible());
    assert_eq!(gpid(&d), Some(2));
}
#[test]
fn manual_overlay_survives_focus_loss_and_game_exit() {
    let mut d = GameDetector::new(true);
    d.toggle();
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS);
    d.on_foreground(fg(9, false));
    assert!(d.visible());
    d.on_process_exited(1);
    assert!(d.visible());
    assert!(d.game().is_none());
}
#[test]
fn auto_overlay_hides_when_game_exits_and_unrelated_exit_is_ignored() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS);
    d.on_process_exited(42);
    assert_eq!(gpid(&d), Some(1));
    d.on_process_exited(1);
    assert!(!d.visible());
}
#[test]
fn auto_show_off_does_not_watch_fullscreen_while_hidden() {
    let mut d = GameDetector::new(false);
    d.on_foreground(fg(1, true));
    assert_eq!(d.candidate_pid(), None);
    assert!(!d.visible());
}
#[test]
fn anchor_prefers_game_window() {
    let mut d = GameDetector::new(true);
    assert_eq!(d.anchor_hwnd(), None);
    d.on_foreground(fg(5, false));
    assert_eq!(d.anchor_hwnd(), Some(50));
    d.on_foreground(fg(1, true)); d.on_presenting(1, CONFIRM_MS);
    d.on_foreground(fg(5, false));
    assert_eq!(d.anchor_hwnd(), Some(10));
}

// process_info.rs
#[test]
fn ignores_shell_browsers_and_self() {
    for exe in ["explorer.exe", "DWM.EXE", "msedge.exe", "chrome.exe", "firefox.exe", "brave.exe",
                "opera.exe", "vivaldi.exe", "msedgewebview2.exe", "Perf-Overlay.exe"] {
        assert!(is_ignored(exe, "perf-overlay.exe"), "{exe}");
    }
    assert!(!is_ignored("witcher3.exe", "perf-overlay.exe"));
}
#[test]
fn display_name_fallback_chain() {
    assert_eq!(display_name(Some("The Witcher 3"), Some("The Witcher 3: Wild Hunt"), Some("witcher3.exe")), "The Witcher 3");
    assert_eq!(display_name(Some("  "), Some("Fake Game"), Some("fake_game.exe")), "Fake Game");
    assert_eq!(display_name(None, None, Some("game.exe")), "game");
    assert_eq!(display_name(None, Some(""), None), "Jogo");
}

// effects.rs
fn all() -> Metrics { Metrics { cpu_temp: true, ..Metrics::default() } }
#[test]
fn idle_runs_nothing() {
    assert_eq!(plan_effects(false, false, false, &all()), Effects::default());
}
#[test]
fn effects_by_situation() {
    assert_eq!(plan_effects(true, false, false, &all()), Effects { etw: true, sample_timer: true, graph_timer: true, detect_timer: false });
    let no_frames = Metrics { fps: false, graph: false, ..all() };
    assert_eq!(plan_effects(true, false, false, &no_frames), Effects { etw: false, sample_timer: true, graph_timer: false, detect_timer: false });
    assert_eq!(plan_effects(false, false, true, &all()), Effects { etw: true, sample_timer: false, graph_timer: false, detect_timer: true });
    assert_eq!(plan_effects(false, true, false, &all()), Effects { etw: true, sample_timer: true, graph_timer: false, detect_timer: false });
}
#[test]
fn effective_metrics_drop_unavailable_and_driverless() {
    let mut a = Availability::all_ok();
    a.gpu_temp = Avail::NoSensor { reason: "x".into() };
    let e = effective_metrics(&all(), &a, DriverState::Missing);
    assert!(!e.gpu_temp && !e.cpu_temp && e.fps && e.gpu_use);
    assert!(effective_metrics(&all(), &Availability::all_ok(), DriverState::Installed).cpu_temp);
    a.fps = Avail::NoSensor { reason: "x".into() };
    assert!(!effective_metrics(&all(), &a, DriverState::Installed).graph);
}
```

- [ ] **Step 2:** Run: `cargo test -p overlay-engine game process_info effects` → Expected: FAIL (não compila).
- [ ] **Step 3:** Implementar conforme as regras acima.
- [ ] **Step 4:** Run: `cargo test -p overlay-engine` → Expected: PASS.
- [ ] **Step 5:** Commit: `feat(engine): add game detection state machine and effect planning`

---

### Task 10: Overlay — formatação, layout e posição

**Files:**
- Create: `crates/engine/src/overlay/{mod,colors,format,layout,position}.rs`

**Interfaces:**
- Consumes: `settings::{Style, Pos, Metrics}`, `model::MetricsSample`, `frames::stats::downsample_max`.
- Produces:
  - `colors.rs`:
    - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct Rgba { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }`
    - Constantes com os valores das Global Constraints: `WHITE`, `WHITE_65` (a = 166), `GPU_LABEL`, `CPU_LABEL`, `GRAPH`, `GPU_BAR`, `CPU_BAR`, `BAR_TRACK` (a = 36), `SHADOW` (a = 217).
    - `pub fn background(opacity_pct: u32) -> Rgba`: rgb(12,12,18) com `a = round(opacity × 255 / 100)`.
  - `format.rs`:
    - `pub fn fmt_fps(v: Option<u32>) -> String` e `pub fn fmt_ms(v: Option<f32>) -> String`: uma casa decimal com vírgula; `None` vira "—".
    - `pub fn fmt_pair(use_on: bool, use_v: Option<u8>, temp_on: bool, temp_v: Option<u8>, style: Style) -> String`: partes "96%" e "71°" ("—" quando falta o valor), unidas por `" "` no estilo Linha e `" · "` no Card.
  - `layout.rs`:
    - `pub trait TextMeasure { fn width(&self, text: &str, size: f32, weight: u16) -> f32; }`
    - `#[derive(Clone, Debug, PartialEq)] pub enum Prim { RoundRect { x: f32, y: f32, w: f32, h: f32, r: f32, color: Rgba }, Text { x: f32, y: f32, size: f32, weight: u16, color: Rgba, text: String }, Polyline { points: Vec<(f32, f32)>, width: f32, color: Rgba } }`. O `y` do `Text` é o topo da caixa de linha (altura = size × 1,25). Todo `Text` leva sombra, desenhada pelo renderer.
    - `pub struct Layout { pub w: f32, pub h: f32, pub prims: Vec<Prim> }`, em px lógicos a 100%.
    - `pub fn layout(style: Style, metrics: &Metrics, sample: &MetricsSample, opacity: u32, m: &dyn TextMeasure) -> Layout`. `metrics` já vem efetivo. O Card usa as 126 colunas de `sample.graph` direto; a Linha usa `downsample_max(&sample.graph, 44)`.
  - `position.rs`:
    - `#[derive(Clone, Copy, Debug, PartialEq)] pub struct Rect { pub x: i32, pub y: i32, pub w: i32, pub h: i32 }`
    - `pub const MARGIN_PX: f32 = 12.0;`
    - `pub fn place(monitor: Rect, pos: Pos, w: i32, h: i32, dpi_scale: f32) -> (i32, i32)`: margem `round(12 × dpi_scale)` a partir de `rcMonitor`; posições do meio centralizam com divisão inteira.

Medidas a 100% (do handoff, seção "Prévia = overlay real"):
- **Linha:** pílula com altura 26, raio 13, padding horizontal 12 e gap 12 entre itens. Itens, na ordem e só os ligados:
  - FPS: número 12/600 branco + 3 px + "FPS" 9,5/600 `WHITE_65`;
  - gráfico 44×14, traço 1,4;
  - GPU: "GPU" 12/600 `GPU_LABEL` + 5 px + valor 12/600 branco;
  - CPU: idem com `CPU_LABEL`.
  - Sem nenhum item: "Nenhuma métrica" 12/500 `WHITE_65`.
- **Card:** largura 148, padding 9/11/10, gap 6 entre linhas. Linhas, só as ligadas:
  - FPS (altura 26): número 26/700 + 4 px + "FPS" 10/600 `WHITE_65`, e "`fmt_ms` ms" 10,5/400 `WHITE_65` alinhado à direita em 137;
  - gráfico 126×26, traço 1,5;
  - GPU e CPU: colunas 26 | flex | auto com gap 6, texto 10,5/600; barra de 5 px de altura e raio 3, centralizada (trilho `BAR_TRACK` + preenchimento `uso/100`), que só aparece com uso ligado, mantendo o espaço; valor alinhado à direita em 137.
  - Sem nenhuma linha: "Nenhuma métrica ativa" 11/500 `WHITE_65`.
- **Gráfico:** `x_i = i·w/(n−1)` e `y_i = h − 1 − min(1, v/graph_max_ms)·(h − 2)`, relativos à caixa do gráfico. Com menos de 2 pontos, não desenha polyline.

- [ ] **Step 1: Escrever os testes**

```rust
// layout.rs
struct Fake;
impl TextMeasure for Fake { fn width(&self, t: &str, size: f32, _: u16) -> f32 { t.chars().count() as f32 * size * 0.5 } }
fn sample() -> MetricsSample {
    MetricsSample { fps: Some(142), frametime_ms: Some(6.9), graph: vec![7.0; 126], graph_max_ms: 8.0,
                    gpu_use: Some(96), gpu_temp: Some(71), cpu_use: Some(38), cpu_temp: Some(64) }
}
fn all_on() -> Metrics { Metrics { cpu_temp: true, ..Metrics::default() } }
fn texts(l: &Layout) -> Vec<String> { l.prims.iter().filter_map(|p| match p { Prim::Text { text, .. } => Some(text.clone()), _ => None }).collect() }
fn right_edge(l: &Layout, s: &str) -> f32 {
    l.prims.iter().find_map(|p| match p { Prim::Text { x, text, size, weight, .. } if text == s => Some(x + Fake.width(text, *size, *weight)), _ => None }).unwrap()
}

#[test]
fn line_pill_is_26_high_with_all_items() {
    let l = layout(Style::Line, &all_on(), &sample(), 80, &Fake);
    assert_eq!(l.h, 26.0);
    match &l.prims[0] { Prim::RoundRect { r, h, color, .. } => assert_eq!((*r, *h, *color), (13.0, 26.0, background(80))), p => panic!("{p:?}") }
    let t = texts(&l);
    for want in ["142", "FPS", "GPU", "96% 71°", "CPU", "38% 64°"] { assert!(t.contains(&want.to_string()), "{want}"); }
    assert_eq!(l.prims.iter().filter(|p| matches!(p, Prim::Polyline { .. })).count(), 1);
}
#[test]
fn card_is_148_wide_and_shows_ms() {
    let l = layout(Style::Card, &all_on(), &sample(), 80, &Fake);
    assert_eq!(l.w, 148.0);
    assert!(texts(&l).contains(&"6,9 ms".to_string()));
    assert!(texts(&l).contains(&"96% · 71°".to_string()));
}
#[test]
fn card_temp_only_keeps_value_position_without_bar() {
    let with = layout(Style::Card, &all_on(), &sample(), 80, &Fake);
    let without = layout(Style::Card, &Metrics { gpu_use: false, ..all_on() }, &sample(), 80, &Fake);
    assert_eq!(right_edge(&with, "96% · 71°"), right_edge(&without, "71°"));
    let rects = |l: &Layout| l.prims.iter().filter(|p| matches!(p, Prim::RoundRect { .. })).count();
    assert_eq!(rects(&with), rects(&without) + 2);
    assert_eq!(with.h, without.h);
}
#[test]
fn placeholders_when_no_metrics() {
    let none = Metrics { fps: false, graph: false, gpu_use: false, gpu_temp: false, cpu_use: false, cpu_temp: false };
    assert!(texts(&layout(Style::Line, &none, &sample(), 80, &Fake)).contains(&"Nenhuma métrica".to_string()));
    assert!(texts(&layout(Style::Card, &none, &sample(), 80, &Fake)).contains(&"Nenhuma métrica ativa".to_string()));
}
#[test]
fn graph_points_stay_inside_the_layout() {
    let mut s = sample();
    s.graph = vec![1.0, 30.0, 7.0];
    for style in [Style::Line, Style::Card] {
        let l = layout(style, &all_on(), &s, 80, &Fake);
        let pts = l.prims.iter().find_map(|p| match p { Prim::Polyline { points, .. } => Some(points.clone()), _ => None }).unwrap();
        assert!(pts.iter().all(|&(x, y)| (0.0..=l.w).contains(&x) && (0.0..=l.h).contains(&y)), "{style:?}");
    }
}
// format.rs
#[test]
fn format_values_pt_br() {
    assert_eq!(fmt_ms(Some(6.94)), "6,9");
    assert_eq!(fmt_ms(None), "—");
    assert_eq!(fmt_fps(Some(142)), "142");
    assert_eq!(fmt_fps(None), "—");
    assert_eq!(fmt_pair(true, Some(96), true, Some(71), Style::Line), "96% 71°");
    assert_eq!(fmt_pair(true, Some(96), true, Some(71), Style::Card), "96% · 71°");
    assert_eq!(fmt_pair(false, None, true, Some(64), Style::Card), "64°");
    assert_eq!(fmt_pair(true, None, true, Some(64), Style::Card), "— · 64°");
}
// colors.rs
#[test]
fn colors_follow_handoff() {
    assert_eq!(GPU_LABEL, Rgba { r: 0xB6, g: 0xA8, b: 0xFF, a: 255 });
    assert_eq!(CPU_BAR, Rgba { r: 0x52, g: 0xA9, b: 0xFE, a: 255 });
    assert_eq!(background(80).a, 204);
    assert_eq!(background(30).a, 77);
}
// position.rs
#[test]
fn place_corners_and_edges() {
    let m = Rect { x: 0, y: 0, w: 1920, h: 1080 };
    assert_eq!(place(m, Pos::Tr, 148, 100, 1.0), (1920 - 12 - 148, 12));
    assert_eq!(place(m, Pos::Bc, 148, 100, 1.0), ((1920 - 148) / 2, 1080 - 12 - 100));
    assert_eq!(place(m, Pos::Ml, 148, 100, 1.0), (12, (1080 - 100) / 2));
}
#[test]
fn place_on_left_monitor_with_150_percent_dpi() {
    let m = Rect { x: -2560, y: 0, w: 2560, h: 1440 };
    assert_eq!(place(m, Pos::Tl, 222, 150, 1.5), (-2560 + 18, 18));
    assert_eq!(place(m, Pos::Br, 222, 150, 1.5), (-18 - 222, 1440 - 18 - 150));
}
```

- [ ] **Step 2:** Run: `cargo test -p overlay-engine overlay` → Expected: FAIL (não compila).
- [ ] **Step 3:** Implementar conforme as medidas acima.
- [ ] **Step 4:** Run: `cargo test -p overlay-engine overlay` → Expected: PASS (10 tests).
- [ ] **Step 5:** Commit: `feat(engine): add overlay layout, formatting and placement`

---

### Task 11: Renderer Direct2D (software) e janela do overlay

**Files:**
- Create: `crates/engine/src/overlay/render.rs`, `crates/engine/src/overlay/window.rs`, `crates/engine/examples/overlay_demo.rs`, `scripts/screenshot.ps1`
- Modify: `crates/engine/src/native/mod.rs` (variante `Overlay`; `WM_DISPLAYCHANGE` da janela do overlay gera `NativeEvent::DisplayChanged` e reposiciona), `crates/engine/Cargo.toml` (features Direct2D, DirectWrite, Gdi, HiDpi)

**Interfaces:**
- Consumes: `layout`, `Prim`, `TextMeasure`, `place`, `colors` (Task 10); `NativeHandle` (Task 4).
- Produces:
  - `#[derive(Clone, Debug, PartialEq)] pub struct OverlayConfig { pub style: Style, pub pos: Pos, pub size: u32, pub opacity: u32, pub metrics: Metrics }`
  - `pub enum OverlayCmd { Configure(OverlayConfig), Data(MetricsSample), Show(bool), Anchor(Option<isize>), ReassertTopmost }`, e `NativeCmd::Overlay(OverlayCmd)`.
  - `pub struct Renderer`, com `new() -> windows::core::Result<Self>` e `render(&mut self, layout: &Layout, scale: f32) -> windows::core::Result<Frame>`; `impl TextMeasure for Renderer` via `IDWriteTextLayout`.
  - `pub struct Frame { pub w: i32, pub h: i32, /* DIB 32bpp + memDC */ }`, com `pixel(&self, x: i32, y: i32) -> [u8; 4]` em BGRA pré-multiplicado.
  - `pub struct OverlayWindow`, com `create() -> windows::core::Result<Self>`, `handle(&mut self, cmd: OverlayCmd)` e `ex_style(&self) -> u32`.

Desenho:
- **Renderer:** `ID2D1Factory` single-thread e `ID2D1DCRenderTarget` com `D2D1_RENDER_TARGET_TYPE_SOFTWARE`, `B8G8R8A8_UNORM` e `D2D1_ALPHA_MODE_PREMULTIPLIED`, ligado a uma DIB top-down de `ceil(w·scale)×ceil(h·scale)`. O desenho começa com `Clear` transparente e `SetTransform(scale)`. O texto usa a fonte "Segoe UI Variable Display" com a feature `tnum` (`IDWriteTypography`) e antialias de texto `GRAYSCALE`. Cada `Text` é desenhado primeiro com `SHADOW` em (x, y+1) e depois na cor.
- **Janela:**
  - Criada com `WS_POPUP` e `WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`.
  - Atualizada com `UpdateLayeredWindow(.., ULW_ALPHA)`.
  - Escala = `size/100 × GetDpiForMonitor(MDT_EFFECTIVE_DPI)/96` do monitor do `Anchor` (`MonitorFromWindow(.., MONITOR_DEFAULTTONEAREST)`; sem âncora, usa `GetForegroundWindow()`), posicionada com `place(rcMonitor, ..)`.
  - Só redesenha quando chega `Configure` ou `Data`, ou quando a âncora muda de monitor.
  - `Show(false)` faz `ShowWindow(SW_HIDE)`; `Show(true)` faz `SW_SHOWNOACTIVATE`.
  - `ReassertTopmost` chama `SetWindowPos(HWND_TOPMOST, .., SWP_NOMOVE|SWP_NOSIZE|SWP_NOACTIVATE)`.
- **Exemplos:** `overlay_demo` e `engine_demo` chamam `SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)` logo no início, ignorando erro, porque exemplos não têm manifesto.

- [ ] **Step 1: Escrever os testes**

```rust
// render.rs
fn sample() -> MetricsSample {
    MetricsSample { fps: Some(142), frametime_ms: Some(6.9), graph: vec![7.0; 126], graph_max_ms: 8.0,
                    gpu_use: Some(96), gpu_temp: Some(71), cpu_use: Some(38), cpu_temp: Some(64) }
}
#[test]
fn renders_card_with_transparent_corner_and_background_alpha() {
    let mut r = Renderer::new().unwrap();
    let l = layout(Style::Card, &Metrics { cpu_temp: true, ..Metrics::default() }, &sample(), 80, &r);
    let f = r.render(&l, 1.0).unwrap();
    assert_eq!((f.w, f.h), (148, l.h.ceil() as i32));
    assert_eq!(f.pixel(0, 0)[3], 0);
    let a = f.pixel(74, f.h - 3)[3];
    assert!((202..=206).contains(&a), "alpha {a}");
    assert_eq!(r.render(&l, 1.5).unwrap().w, (148.0f32 * 1.5).ceil() as i32);
}
// window.rs
#[test]
fn overlay_window_is_click_through_and_never_activates() {
    let w = OverlayWindow::create().unwrap();
    let need = (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE).0;
    assert_eq!(w.ex_style() & need, need);
}
```

- [ ] **Step 2:** Run: `cargo test -p overlay-engine render window` → Expected: FAIL (não compila).
- [ ] **Step 3:** Implementar `Renderer`, `Frame`, `OverlayWindow` e a variante `NativeCmd::Overlay`. Na thread nativa, `NativeCmd::Overlay(cmd)` chama `overlay.handle(cmd)`.
- [ ] **Step 4:** Run: `cargo test -p overlay-engine` → Expected: PASS.
- [ ] **Step 5: `overlay_demo`.** Flags: `--style line|card --pos tr --size 100 --opacity 80 --seconds N`. Mostra o overlay com valores de exemplo que mudam a cada 500 ms, como no `tick()` do protótipo, e Ctrl+Shift+O alterna a visibilidade.
- [ ] **Step 6: `scripts/screenshot.ps1`.** Parâmetro `-Out file.png`. Chama `SetProcessDPIAware` via `Add-Type` e depois `Graphics.CopyFromScreen` da tela virtual inteira.
- [ ] **Step 7: Conferência visual.** Pra cada estilo:
  - Rode `overlay_demo --style card --pos tr --seconds 8` em segundo plano e, depois de 2 s, `scripts\screenshot.ps1 -Out $env:TEMP\ov-card.png`; abra a imagem com Read. Repita com `--style line --pos tc`.
  - Expected: canto e posição corretos, cores e medidas iguais às da prévia do protótipo.
  - Se notar diferença de alinhamento, ajuste o layout (Task 10) e rode os testes de novo.
- [ ] **Step 8: Monitor e DPI** (Review Focus 1).
  - Confirme que o overlay se reposiciona ao receber `WM_DISPLAYCHANGE`: mude a escala do Windows entre 100% e 125% com o demo rodando e veja que o overlay continua no canto, com o tamanho proporcional.
  - Com um monitor só, registre "multi-monitor não testado nesta máquina" no relatório.
- [ ] **Step 9:** Commit: `feat(engine): render overlay with software direct2d into a layered window`

---

### Task 12: Controller — orquestração, foco, fim de processo e demo do engine

**Files:**
- Create: `crates/engine/src/controller.rs`, `crates/engine/src/native/foreground.rs`, `crates/engine/src/native/process_watch.rs`, `crates/engine/examples/engine_demo.rs`
- Modify: `crates/engine/src/native/mod.rs` (`SetWinEventHook(EVENT_SYSTEM_FOREGROUND, .., WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS)` na thread nativa, que gera `NativeEvent::Foreground(ForegroundInfo)`), `crates/engine/src/lib.rs`

**Interfaces:**
- Consumes: tudo das Tasks 2–11.
- Produces:
  - `foreground.rs`:
    - `#[derive(Clone, Debug)] pub struct ForegroundInfo { pub hwnd: isize, pub pid: u32, pub exe_name: String, pub title: String, pub covers_monitor: bool }`
    - `pub fn covers_monitor(window: Rect, monitor: Rect) -> bool`: tolera 1 px de folga.
    - `pub fn describe(hwnd: isize) -> Option<ForegroundInfo>`
    - `pub fn is_window_alive(hwnd: isize) -> bool`
  - `process_watch.rs`: `pub struct ExitWatch` (o `Drop` cancela) e `pub fn watch_exit(pid: u32, on_exit: Box<dyn FnOnce() + Send>) -> Option<ExitWatch>`. Usa `OpenProcess(SYNCHRONIZE)` + `RegisterWaitForSingleObject(WT_EXECUTEONLYONCE)` e devolve `None` se não conseguir abrir o processo.
  - `controller.rs`:
    - `pub trait UiSink: Send + Sync { fn emit(&self, event: &str, payload: serde_json::Value); }`
    - `pub fn startup_hotkey_error(combo: &[String], r: &Result<(), HotkeyError>) -> Option<String>`
    - `pub struct Controller`, com:
      - Ciclo: `new(settings: Settings, settings_path: PathBuf, sink: Arc<dyn UiSink>) -> Arc<Controller>`, `start(self: &Arc<Self>)`, `shutdown(&self)`.
      - Estado e configurações: `state(&self) -> AppState`, `update_settings(&self, patch: serde_json::Value) -> Result<Settings, String>`.
      - Overlay: `toggle_overlay(&self)`, `set_overlay_visible(&self, visible: bool)`.
      - Atalho: `begin_hotkey_capture(&self)`, `end_hotkey_capture(&self)`, `try_set_hotkey(&self, combo: Vec<String>) -> HotkeyResult`.
      - Driver: `install_driver(self: &Arc<Self>)`.
      - Janela: `set_config_open(&self, open: bool)`.
  - Eventos emitidos (nome → payload): `"status"` → `Status`; `"metrics"` → `MetricsSample` (a cada 500 ms, só com a configuração aberta); `"availability"` → `Availability`; `"driver"` → `DriverState`; `"settings"` → `Settings`.

Comportamento (spec 4.1, 4.5 e 4.6):
- **`start`:**
  - Cria `Sensors`, `FrameShared` e o `NativeHandle`, cujo `on_event` chama `on_native_event` com um `Weak` do controller.
  - Registra o atalho e guarda `startup_hotkey_error`.
  - Manda `Overlay(Configure(..))` com as métricas efetivas.
  - Define o driver: `Installed` se `pawnio::is_installed()`, senão `Missing`.
- **Eventos nativos:**
  - `Hotkey` → `detector.toggle()`.
  - `Foreground(info)`: primeiro, se há jogo e `!is_window_alive(jogo.hwnd)`, chama `detector.on_process_exited(jogo.pid)` (Review Focus 2). Depois `detector.on_foreground(Fg { ignored: process_info::is_ignored(&info.exe_name, <nome do exe atual>), .. })` e `Overlay(ReassertTopmost)`.
  - `Timer(Detect)` → `detector.on_presenting(c, observer.continuous_ms(c, now_ns()))` pro candidato `c`.
  - `Timer(Sample)` → `sensors.sample(efetivas, jogo)` + FPS e frametime do tracker + gráfico de 126 colunas e `graph_y_max`. Manda `Overlay(Data)` e, se a configuração estiver aberta, emite `"metrics"`.
  - `Timer(Graph)` → atualiza só `graph` e `graph_max_ms` da última amostra e manda `Overlay(Data)`.
- **`sync()`, depois de toda mudança:**
  1. Se o jogo mudou: `frames.set_target`, `game_name` via `process_info`, `ExitWatch` novo cujo callback chama `detector.on_process_exited`.
  2. `plan_effects(..)`.
  3. Liga ou desliga a `FrameCapture`. Se der `AccessDenied`, marca `availability.fps = NoSensor(REASON_NEEDS_ADMIN)` e emite `"availability"`.
  4. `SetTimers`.
  5. Manda `Show` e `Anchor` ao overlay.
  6. Emite `"status"` se o status mudou.
- **Trava:** **nunca** segure a trava interna do controller enquanto chama `NativeHandle::register_hotkey`, porque a thread nativa pode estar esperando essa trava em `on_native_event` e o resultado é deadlock. Copie o que precisa, solte a trava, chame, e trave de novo.
- **`try_set_hotkey`:**
  - Erro de `parse`, ou conflito conhecido, ou `register_hotkey` que falhou: re-registra o atalho antigo e devolve `{ ok: false, error: Some(err.message(&combo)) }`.
  - Sucesso: salva `shortcut`, zera `hotkey_error` e emite `"settings"`.
- **Captura de atalho:** `begin_hotkey_capture` manda `UnregisterHotkey`; `end_hotkey_capture` registra de novo o atalho salvo.
- **`update_settings`:** `apply_patch`, salva, aplica (`detector.set_auto_show`, `Overlay(Configure)`), `sync()`, emite `"settings"` e devolve as configurações.
- **`install_driver`:** numa thread: `driver = Installing`, emite `"driver"`. Depois chama `pawnio::install::run_install()`.
  - Sucesso: `sensors.reload_cpu_temp()`, `driver = Installed`, `metrics.cpu_temp = true`, salva, emite `"availability"`, `"settings"` e `"driver"`.
  - Falha: `driver = Failed` e emite `"driver"`.
- **Sem `start`:** os métodos funcionam sem a thread nativa (ignoram os envios), pra permitir os testes abaixo.

- [ ] **Step 1: Escrever os testes**

```rust
// foreground.rs
#[test]
fn covers_monitor_allows_one_pixel_slack() {
    let m = Rect { x: 0, y: 0, w: 1920, h: 1080 };
    assert!(covers_monitor(Rect { x: -1, y: -1, w: 1922, h: 1082 }, m));
    assert!(covers_monitor(m, m));
    assert!(!covers_monitor(Rect { x: 0, y: 0, w: 1920, h: 1040 }, m));
}
// process_watch.rs
#[test]
fn watch_exit_fires_when_process_ends() {
    let child = std::process::Command::new("cmd").args(["/c", "ping -n 2 127.0.0.1 >nul"]).spawn().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let _w = watch_exit(child.id(), Box::new(move || { let _ = tx.send(()); })).unwrap();
    assert!(rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok());
}
// controller.rs
struct Rec(std::sync::Mutex<Vec<String>>);
impl UiSink for Rec { fn emit(&self, e: &str, _: serde_json::Value) { self.0.lock().unwrap().push(e.into()); } }
fn s(v: &[&str]) -> Vec<String> { v.iter().map(|x| x.to_string()).collect() }

#[test]
fn startup_hotkey_error_message() {
    assert_eq!(startup_hotkey_error(&s(&["Ctrl", "Shift", "O"]), &Err(HotkeyError::InUse)).as_deref(),
               Some("Ctrl + Shift + O já é usado por outro app. Escolha outra."));
    assert_eq!(startup_hotkey_error(&s(&["Ctrl", "Shift", "O"]), &Ok(())), None);
}
#[test]
fn update_settings_persists_and_rejects_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let sink = std::sync::Arc::new(Rec(Default::default()));
    let c = Controller::new(Settings::default(), path.clone(), sink.clone());
    assert!(!c.state().status.overlay_visible);
    assert_eq!(c.update_settings(serde_json::json!({"size": 150})).unwrap().size, 150);
    assert_eq!(crate::settings::load(&path).0.size, 150);
    assert!(sink.0.lock().unwrap().contains(&"settings".to_string()));
    assert!(c.update_settings(serde_json::json!({"pos": "center"})).is_err());
    assert_eq!(c.state().settings.pos, Pos::Tr);
}
```

- [ ] **Step 2:** Run: `cargo test -p overlay-engine controller foreground process_watch` → Expected: FAIL (não compila).
- [ ] **Step 3:** Implementar `foreground.rs`, `process_watch.rs`, o hook de foco na thread nativa e o `Controller`.
- [ ] **Step 4:** Run: `cargo test -p overlay-engine` → Expected: PASS.
- [ ] **Step 5: `engine_demo`.**
  - Flags: `--seconds N --settings <path> --log <path>`.
  - Cria um `Controller` com um `UiSink` que grava cada evento como linha JSON `{"event":..,"payload":..}` no log e chama `start()`.
  - Após N segundos chama `shutdown()`. Ctrl+Shift+O funciona.
- [ ] **Step 6 [ADMIN]: Cenário com o jogo falso.**
  - Apague `$env:TEMP\demo-settings.json`.
  - Rode `scripts\run-elevated.ps1 -Exe target\debug\examples\engine_demo.exe -ArgList "--seconds 40 --settings $env:TEMP\demo-settings.json --log $env:TEMP\demo.log" -Out $env:TEMP\demo-out.txt` **em segundo plano** (a chamada bloqueia até o fim, então use um job do PowerShell).
  - 3 s depois, `Start-Process target\debug\examples\fake_game.exe -ArgumentList "--fps 60 --seconds 10"`; 5 s depois, tire um screenshot.
  - Expected no screenshot: o Card no canto superior direito, por cima do jogo falso, com FPS entre 57 e 63.
  - Expected em `demo.log`: um `status` com `{"overlayVisible":true,"gameName":"Fake Game"}` e, depois do fim do jogo falso, `{"overlayVisible":false,"gameName":null}`.
- [ ] **Step 7: Ocioso de verdade.** Ainda dentro dos 40 s, depois do fim do jogo falso, rode `scripts\measure.ps1 -Name engine_demo -Seconds 10`. Expected: `cpu_pct_avg` menor que 0,1.
- [ ] **Step 8:** Commit: `feat(engine): orchestrate detection, capture, sensors and overlay in controller`

---

### Task 13: Casca Tauri — bandeja, janela de configuração, comandos e eventos

**Files:**
- Create: `src-tauri/src/commands.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/config_window.rs`, `src-tauri/src/sink.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/capabilities/default.json`, `src/main.tsx` (página provisória)

**Interfaces:**
- Consumes: `Controller`, `UiSink`, `settings::{load, settings_path, LoadOutcome}` (engine).
- Produces:
  - Comandos Tauri (os argumentos chegam do JS em camelCase):
    - `get_state() -> AppState`
    - `update_settings(patch: serde_json::Value) -> Result<Settings, String>`
    - `set_overlay_visible(visible: bool)`
    - `begin_hotkey_capture()`, `end_hotkey_capture()`
    - `try_set_hotkey(combo: Vec<String>) -> HotkeyResult`
    - `install_driver()`
  - `config_window::open(app: &AppHandle)`
  - `sink::TauriSink(AppHandle)`, que implementa `UiSink` com `app.emit(event, payload)`.

Comportamento:
- **`setup`:**
  - `let (settings, outcome) = load(&settings_path())`.
  - Cria e inicia o `Controller` (guardado em `app.manage(Arc<Controller>)`).
  - Monta a bandeja.
  - Se `outcome == LoadOutcome::Missing || !settings.general.minimized`, abre a configuração. Primeira execução sempre abre (spec 4.1).
- **Bandeja:**
  - Menu "Configurações" (`open`), "Mostrar/esconder overlay" (`controller.toggle_overlay()`), separador, "Sair" (`controller.shutdown()` e depois `app.exit(0)`).
  - Clique esquerdo solto (`TrayIconEvent::Click { button: Left, button_state: Up }`) abre a configuração. O menu só abre com o botão direito.
- **Single-instance:** o callback chama `config_window::open`.
- **`open`:**
  - Se a janela "config" existe: `unminimize`, `show`, `set_focus`.
  - Senão, cria `WebviewWindowBuilder::new(app, "config", WebviewUrl::App("index.html".into()))` com `inner_size(480.0, 680.0)`, `resizable(false)`, `maximizable(false)`, `decorations(false)`, `shadow(true)` (o Win11 arredonda os cantos), `center()` e `title("Overlay de desempenho")`, e chama `controller.set_config_open(true)`.
  - Em `WindowEvent::Destroyed`, chama `controller.set_config_open(false)`.
  - Fechar destrói a janela (comportamento padrão do `close()` no Tauri v2).
- **Capabilities da janela `config`:** `core:default`, `core:event:default`, `core:window:allow-minimize`, `core:window:allow-close`, `core:window:allow-start-dragging`.
- **Página provisória** (`src/main.tsx`): chama `invoke('get_state')`, mostra o JSON num `<pre>` e reconsulta a cada evento `status`. A Task 14 substitui.

- [ ] **Step 1:** Implementar conforme o comportamento acima.
- [ ] **Step 2:** Run: `cargo build -p perf-overlay && cargo test --workspace` → Expected: build OK, testes PASS.
- [ ] **Step 3: Fluxo da bandeja.**
  - Rode `npx tauri dev`.
  - Expected: sem `settings.json`, a janela abre na primeira execução. Fechar mantém o app na bandeja. O clique esquerdo na bandeja reabre. Uma segunda execução (`Start-Process target\debug\perf-overlay.exe`) abre a janela da instância existente. "Mostrar/esconder overlay" mostra e esconde o Card.
  - Com a janela aberta, Ctrl+Shift+O muda `overlayVisible` no JSON.
- [ ] **Step 4: A memória volta ao fechar.**
  - Abra e feche a configuração. Espere 10 s e rode `scripts\measure.ps1 -Name perf-overlay -Seconds 5`.
  - Expected: `webview_children=0` e `private_ws_mb` dentro do orçamento de oculto.
  - Se sobrarem processos do WebView2, investigue antes de seguir: tem que haver nenhum.
- [ ] **Step 5:** Commit: `feat: wire tauri tray, config window lifecycle and ipc to controller`

---

### Task 14: Tela de configuração, parte 1 — layout, prévia e controles

**Files:**
- Create: `src/types.ts`, `src/ipc.ts`, `src/mock.ts`, `src/format.ts`, `src/format.test.ts`, `src/theme.css`, `src/App.tsx`, `src/components/{TitleBar,StatusRow,Preview,OverlayLine,OverlayCard,MetricPills,Segmented,Slider,GeneralToggles}.tsx`
- Modify: `src/main.tsx`, `index.html`

**Interfaces:**
- Consumes: comandos e eventos da Task 13; tipos do `model.rs` (Task 3).
- Produces:
  - `types.ts`: espelho exato dos tipos Rust em camelCase:

```ts
export type Style = 'line' | 'card';
export type Pos = 'tl' | 'tc' | 'tr' | 'ml' | 'mr' | 'bl' | 'bc' | 'br';
export interface Metrics { fps: boolean; graph: boolean; gpuUse: boolean; gpuTemp: boolean; cpuUse: boolean; cpuTemp: boolean }
export interface Settings { version: number; metrics: Metrics; style: Style; pos: Pos; size: number; opacity: number; shortcut: string[]; general: { startup: boolean; minimized: boolean; autoShow: boolean } }
export interface Status { overlayVisible: boolean; gameName: string | null }
export type Avail = { state: 'ok' } | { state: 'noSensor'; reason: string };
export interface Availability { fps: Avail; gpuUse: Avail; gpuTemp: Avail; cpuUse: Avail; cpuTemp: Avail }
export type DriverState = 'missing' | 'installing' | 'failed' | 'installed';
export interface MetricsSample { fps: number | null; frametimeMs: number | null; graph: number[]; graphMaxMs: number; gpuUse: number | null; gpuTemp: number | null; cpuUse: number | null; cpuTemp: number | null }
export interface AppState { settings: Settings; status: Status; availability: Availability; driver: DriverState; hotkeyError: string | null }
export interface HotkeyResult { ok: boolean; error: string | null }
export interface Events { status: Status; metrics: MetricsSample; availability: Availability; driver: DriverState; settings: Settings }
```

  - `ipc.ts`: `export interface Api { getState(): Promise<AppState>; updateSettings(patch: object): Promise<Settings>; setOverlayVisible(v: boolean): Promise<void>; beginHotkeyCapture(): Promise<void>; endHotkeyCapture(): Promise<void>; trySetHotkey(combo: string[]): Promise<HotkeyResult>; installDriver(): Promise<void>; minimize(): Promise<void>; close(): Promise<void>; on<K extends keyof Events>(e: K, cb: (p: Events[K]) => void): Promise<() => void> }` e `export const api: Api`. Dentro do Tauri (`'__TAURI_INTERNALS__' in window`), usa `invoke`, `listen` e `getCurrentWindow()`; no navegador, usa `mock.ts`.
  - `mock.ts`: estado em memória com os padrões da spec e o jogo "The Witcher 3". Gera valores de exemplo a cada 500 ms, como o `tick()` do protótipo. Lê estados da URL: `?driver=missing|failed|installed`, `?igpu=1` (gpuTemp sem sensor), `?nogame=1`, `?hotkeyError=1`, `?installFails=1`.
  - `format.ts`:
    - `fmtFps(v: number | null): string` e `fmtMs(v: number | null): string`
    - `fmtPair(useOn: boolean, useV: number | null, tempOn: boolean, tempV: number | null, style: Style): string`
    - `downsampleMax(v: number[], n: number): number[]`
    - `graphPoints(v: number[], w: number, h: number, yMax: number): string`
    - Todos seguem as mesmas regras da Task 10 e do `pts()` do protótipo.

Regras:
- **Fidelidade:** recrie pixel a pixel tudo do `docs/design-handoff/README.md` exceto a folha do driver, o campo de atalho e os toasts (Task 15). A tabela `POS`, as medidas do overlay, o `fit` da prévia e os textos vêm do protótipo.
- **Barra de título:** a área de arrastar usa `data-tauri-drag-region` (no Tauri v2 isso substitui o `-webkit-app-region`). Os botões chamam `api.minimize()` e `api.close()`.
- **Prévia:** mostra os valores do evento `metrics` quando `status.gameName != null`; senão, valores de exemplo animados (spec 4.3). As flags de exibição seguem as métricas efetivas, como `renderVals()` do protótipo, usando `availability` e `driver`.
- **Mudanças:** cada alteração chama `api.updateSettings(patch)` na hora e adota as configurações devolvidas. O evento `settings` também substitui o estado local.
- **Chip de status:** chama `api.setOverlayVisible(!overlayVisible)`.
- **Atalho na página:** não implemente o toggle por tecla dentro da página, porque o atalho global já é consumido pelo sistema.

- [ ] **Step 1: Escrever os testes**

```ts
// src/format.test.ts
import { expect, test } from 'vitest';
import { fmtFps, fmtMs, fmtPair, downsampleMax, graphPoints } from './format';

test('formata como o overlay real', () => {
  expect(fmtPair(true, 96, true, 71, 'line')).toBe('96% 71°');
  expect(fmtPair(true, 96, true, 71, 'card')).toBe('96% · 71°');
  expect(fmtPair(true, null, true, 64, 'card')).toBe('— · 64°');
  expect(fmtPair(false, null, true, 64, 'card')).toBe('64°');
  expect(fmtMs(6.94)).toBe('6,9');
  expect(fmtMs(null)).toBe('—');
  expect(fmtFps(null)).toBe('—');
});
test('downsampleMax preserva picos', () => {
  expect(downsampleMax([1, 1, 9, 1, 1, 1], 3)).toEqual([1, 9, 1]);
});
test('graphPoints segue o protótipo', () => {
  expect(graphPoints([0, 14], 44, 14, 14)).toBe('0.0,13.0 44.0,1.0');
});
```

- [ ] **Step 2:** Run: `npm test` → Expected: FAIL (módulo `./format` não existe).
- [ ] **Step 3:** Implementar `format.ts` e depois `types.ts`, `ipc.ts`, `mock.ts`, `theme.css` (tokens do handoff, em `oklch` mesmo, que o WebView2 suporta), os componentes e `App.tsx`.
- [ ] **Step 4:** Run: `npm test && npm run build` → Expected: testes PASS, build sem erros de tipo.
- [ ] **Step 5: Comparar com o protótipo.**
  - Rode `npm run dev` e abra `http://localhost:1420` e `docs/design-handoff/Configuracoes Overlay.dc.html` lado a lado, a 100% de zoom e viewport de 480×680 (use a skill claude-in-chrome pra tirar os screenshots).
  - Expected: medidas, cores, fontes e textos iguais em barra de título, status, prévia (8 alvos, nome da posição e o sufixo " · prévia reduzida" com tamanho 200%), pílulas ligadas e desligadas, Linha/Card, sliders (arrastar e setas, passo 5) e opções gerais.
  - Confira `?igpu=1` (pílula tracejada "sem sensor") e `?nogame=1` ("Nenhum jogo detectado").
- [ ] **Step 6: No app real.**
  - Rode `npx tauri dev`.
  - Expected: arrastar pela barra de título funciona, minimizar minimiza, fechar destrói a janela, e mudar estilo, posição, tamanho ou opacidade altera o overlay real na hora (deixe-o visível com Ctrl+Shift+O).
- [ ] **Step 7:** Commit: `feat(ui): recreate settings screen layout, preview and controls`

---

### Task 15: Tela de configuração, parte 2 — atalho, driver e estados

**Files:**
- Create: `src/keys.ts`, `src/keys.test.ts`, `src/components/ShortcutField.tsx`, `src/components/DriverSheet.tsx`, `src/components/Toast.tsx`
- Modify: `src/App.tsx`, `src/components/MetricPills.tsx`

**Interfaces:**
- Consumes: `api` (Task 14); `HotkeyResult`, `DriverState`, `hotkeyError`.
- Produces:
  - `keys.ts`: `keyName(e: Pick<KeyboardEvent, 'code' | 'key'>): string`, `mods(e: Pick<KeyboardEvent, 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>): string[]` e `validateCombo(combo: string[]): string | null`. São cópias exatas de `keyName`, `mods` e `KEYMAP` do protótipo, e produzem os nomes que `hotkey::parse` (Task 2) aceita.

Regras (handoff, seção "Estados"):
- **Captura do atalho:**
  - Clicar no campo chama `api.beginHotkeyCapture()` e entra em captura, com a mesma UI do protótipo.
  - Tecla sem modificador mostra o erro de `validateCombo` e continua capturando.
  - Esc sem modificador, ou perder o foco, cancela e chama `api.endHotkeyCapture()`.
  - Combinação completa: `api.trySetHotkey(combo)`. Com `ok`, mostra o toast "Atalho alterado para <combo com ' + '>". Senão, mostra `error` na cor de erro, com a borda de erro, e mantém o atalho anterior. Nos dois casos a captura termina.
- **`hotkeyError` vindo do backend** (atalho ocupado na inicialização): aparece na dica do campo na cor de erro até um atalho novo ser aceito (Review Focus 4).
- **Pílula "CPU temp."** com `driver !== 'installed'`: sub-rótulo "requer driver", ou "instalação falhou" quando `failed`. Clicar abre a folha.
- **Folha:**
  - "Instalar" chama `api.installDriver()`, e a UI segue o evento `driver`: em `installing`, mostra a barra de progresso **indeterminada**; em `failed`, mostra o estado de falha com "Tentar de novo"; em `installed`, fecha a folha e mostra o toast "Driver instalado. Temperatura da CPU ativada.".
  - Clicar no scrim fecha, exceto durante `installing`.
- **Pílula indisponível** (`availability.<métrica>.state === 'noSensor'`): borda tracejada, "sem sensor", `title` com o `reason`, sem clique.
- **Textos:** os do handoff, sem mudanças.

- [ ] **Step 1: Escrever os testes**

```ts
// src/keys.test.ts
import { expect, test } from 'vitest';
import { keyName, mods, validateCombo } from './keys';

test('keyName mapeia como o protótipo', () => {
  const k = (code: string, key = '') => keyName({ code, key });
  expect(k('KeyO')).toBe('O');
  expect(k('Digit5')).toBe('5');
  expect(k('F12')).toBe('F12');
  expect(k('Numpad3')).toBe('Num 3');
  expect(k('Space')).toBe('Espaço');
  expect(k('ArrowUp')).toBe('↑');
  expect(k('Backquote')).toBe('`');
});
test('mods em ordem fixa', () => {
  expect(mods({ ctrlKey: true, altKey: true, shiftKey: true, metaKey: true })).toEqual(['Ctrl', 'Alt', 'Shift', 'Win']);
});
test('validateCombo exige modificador', () => {
  expect(validateCombo(['O'])).toBe('Use pelo menos Ctrl, Alt ou Shift junto com a tecla.');
  expect(validateCombo(['Ctrl', 'O'])).toBeNull();
});
```

- [ ] **Step 2:** Run: `npm test` → Expected: FAIL (módulo `./keys` não existe).
- [ ] **Step 3:** Implementar `keys.ts` e os componentes conforme as regras.
- [ ] **Step 4:** Run: `npm test && npm run build` → Expected: PASS.
- [ ] **Step 5: Estados no navegador.** Abra o mock com `?driver=missing`, `?driver=failed`, `?installFails=1`, `?hotkeyError=1` e compare com o protótipo: folha, instalando, falhou, sucesso com toast, captura, conflito e erro de modificador.
- [ ] **Step 6: Atalho no app real.**
  - Em `npx tauri dev`, troque o atalho pra Ctrl+Alt+P. Expected: toast; Ctrl+Alt+P alterna o overlay; Ctrl+Shift+O não faz mais nada.
  - Tente Ctrl+Shift+M. Expected: "Ctrl + Shift + M já é usado por Discord. Escolha outra.".
  - Com `cargo run -p overlay-engine --example hold_hotkey -- Ctrl+Alt+Shift+F9 --seconds 60` rodando, tente Ctrl+Alt+Shift+F9. Expected: "… já é usado por outro app. Escolha outra.".
- [ ] **Step 7: Atalho ocupado na inicialização** (Review Focus 4).
  - Volte o atalho pra Ctrl+Shift+O e saia do app.
  - Rode `hold_hotkey -- Ctrl+Shift+O --seconds 60` e inicie o app.
  - Expected: o app sobe, a bandeja funciona, e a tela mostra "Ctrl + Shift + O já é usado por outro app. Escolha outra." no campo do atalho.
- [ ] **Step 8 [ADMIN]: Instalar o driver pela folha.** Só se o PawnIO ainda não estiver instalado **e** o usuário autorizar a instalação: rode o build release e passe pelo fluxo Instalar → toast → CPU temp. na prévia. Senão, registre que o fluxo real não foi validado.
- [ ] **Step 9:** Commit: `feat(ui): add hotkey capture, driver install sheet and unavailable states`

---

### Task 16: Iniciar com o Windows (Agendador de Tarefas)

**Files:**
- Create: `crates/engine/src/autostart.rs`
- Modify: `crates/engine/src/controller.rs`, `crates/engine/src/lib.rs`

**Interfaces:**
- Consumes: `Controller` (Task 12).
- Produces:
  - `pub const TASK_NAME: &str = "PerfOverlay";`
  - `pub fn task_xml(exe: &Path, user_id: &str) -> String`
  - `pub fn enable(exe: &Path) -> Result<(), String>`: grava o XML em `%TEMP%\perf-overlay-task.xml` em UTF-16LE com BOM e roda `schtasks /Create /TN PerfOverlay /XML <arquivo> /F`, sem janela.
  - `pub fn disable() -> Result<(), String>`: `schtasks /Delete /TN PerfOverlay /F`; tarefa inexistente conta como `Ok`.
  - `pub fn is_enabled() -> bool`: `schtasks /Query /TN PerfOverlay` com código de saída 0.

XML (Task Scheduler 1.2):
- `LogonTrigger` com `UserId` = `%USERDOMAIN%\%USERNAME%`.
- `Principal`: `LogonType` `InteractiveToken`, `RunLevel` `HighestAvailable`.
- `Settings`: `MultipleInstancesPolicy` `IgnoreNew`, `DisallowStartIfOnBatteries` false, `StopIfGoingOnBatteries` false, `ExecutionTimeLimit` `PT0S`, `Priority` 4 (prioridade normal; o padrão 7 deixaria o app abaixo do normal).
- `Exec/Command` = caminho do exe com escape XML.

No Controller:
- `start()`: se `general.startup`, chama `enable(current_exe)`. Recriar com `/F` atualiza o caminho e cobre a primeira execução.
- `update_settings`: quando `general.startup` muda, chama `enable` ou `disable`.
- Erros (por exemplo, debug sem admin) só geram `trace!`.

- [ ] **Step 1: Escrever o teste**

```rust
#[test]
fn task_xml_runs_elevated_at_logon_without_limits() {
    let x = task_xml(std::path::Path::new(r"C:\Program Files\A&B\perf-overlay.exe"), r"PC\csequ");
    for want in ["<LogonTrigger>", r"<UserId>PC\csequ</UserId>", "<RunLevel>HighestAvailable</RunLevel>",
                 "<LogonType>InteractiveToken</LogonType>", "<ExecutionTimeLimit>PT0S</ExecutionTimeLimit>",
                 "<DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>",
                 "<StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>", "<Priority>4</Priority>",
                 r"<Command>C:\Program Files\A&amp;B\perf-overlay.exe</Command>"] {
        assert!(x.contains(want), "faltou {want}");
    }
}
```

- [ ] **Step 2:** Run: `cargo test -p overlay-engine autostart` → Expected: FAIL (não compila).
- [ ] **Step 3:** Implementar `autostart.rs` e as chamadas no Controller.
- [ ] **Step 4:** Run: `cargo test --workspace` → Expected: PASS.
- [ ] **Step 5 [ADMIN]: Validar a tarefa.**
  - Com o build release (`npx tauri build --no-bundle`), apague `settings.json` e inicie o app (UAC).
  - Expected: a configuração abre e `schtasks /Query /TN PerfOverlay` encontra a tarefa.
  - Desligue "Iniciar com o Windows". Expected: a consulta falha. Religue. Expected: a tarefa volta.
  - Opcional, se o usuário topar: sair e entrar de novo no Windows e confirmar que o app sobe sem UAC.
- [ ] **Step 6:** Commit: `feat(engine): start with windows through an elevated logon task`

---

### Task 17: Ícone, instalador, medição do orçamento e aceite no The Witcher 3

**Files:**
- Create: `app-icon.svg`, `src-tauri/icons/*` (gerados), `README.md`
- Modify: `src-tauri/tauri.conf.json` (`bundle.targets: ["nsis"]`, `bundle.windows.nsis.installMode: "perMachine"`), `docs/measurements.md`

**Interfaces:**
- Consumes: o app completo.
- Produces: o instalador em `target/release/bundle/nsis/`, a tabela de medições e o README.

- [ ] **Step 1: Ícone.** `app-icon.svg` em 1024×1024: quadrado arredondado em `#A492FB` com três barras verticais brancas de alturas diferentes, um glifo simples de gráfico. Gere os ícones com `npx tauri icon app-icon.svg`.
- [ ] **Step 2 [ADMIN]: Instalador.** Run: `npx tauri build`. Expected: um `.exe` em `target/release/bundle/nsis/`. Instale-o (UAC).
- [ ] **Step 3 [ADMIN]: Medir as três situações da spec 6.**
  - **Oculto:** 60 s depois de iniciar, `measure.ps1 -Seconds 20`.
  - **Overlay visível:** com o `fake_game --fps 60` em tela cheia por 30 s, `measure.ps1 -Seconds 20`.
  - **Configuração:** abra, feche, espere 15 s e meça.
  - Preencha `docs/measurements.md` e compare com o orçamento (ajustado na Task 1, se for o caso). Se alguma linha estourar, pare e investigue antes de seguir.
- [ ] **Step 4: README.md** (pt-BR): o que é; como compilar (`npm install`, `npx tauri build`); como desenvolver (`npm run dev` com o mock, `npx tauri dev`); testes (`cargo test --workspace`, `npm test`); por que precisa de administrador; limitações (tela cheia exclusiva, Vulkan/OpenGL sem FPS neste momento, heurística de detecção).
- [ ] **Step 5: Aceite com o usuário.** Peça pra jogar The Witcher 3 por alguns minutos e conferir:
  1. o overlay aparece sozinho em tela cheia ou janela sem borda;
  2. o FPS bate com o contador da Steam (±2);
  3. GPU % e temperatura batem com o Gerenciador de Tarefas;
  4. a temperatura da CPU é plausível (se o driver estiver instalado);
  5. Alt+Tab esconde e reexibe o overlay; Ctrl+Shift+O esconde de vez até fechar o jogo;
  6. o FPS médio de 30 s com o overlay ligado e desligado não muda de forma perceptível.

  Registre as respostas em `docs/measurements.md`.
- [ ] **Step 6:** Commit: `chore: add icon, nsis bundle, readme and budget measurements`
