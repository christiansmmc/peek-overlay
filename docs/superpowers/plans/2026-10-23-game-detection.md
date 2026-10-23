# Detecção de jogo melhor + "Esconder fora do jogo" — Plano de implementação

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Parar de tratar apps comuns (terminal, chat, editor, launcher) como jogo, deixar o usuário corrigir a detecção ("não é jogo" / "marcar como jogo") e esconder o overlay quando não há jogo em primeiro plano.

**Architecture:** O novo módulo puro `crates/engine/src/game_rules.rs` normaliza caminhos de exe, reconhece pastas de biblioteca de jogos e classifica o app em primeiro plano (`ignored` / `known_game`). `settings.rs` ganha `gameRules` e `general.hideOutsideGame`. O `GameDetector` (puro) troca o caminho "qualquer janela com o overlay visível" por "tela cheia **ou** jogo conhecido", separa a intenção (ligado/desligado) do que é desenhado e aceita uma nomeação explícita. O `Controller` liga tudo, guarda o último app e expõe `mark_game` / `mark_not_game`, que a casca Tauri e a tela usam.

**Tech Stack:** Rust 1.96 (msvc), crate `windows` 0.62 (a feature `Win32_System_Registry` já está ativa), `serde_json`, `tempfile` (já é dependência normal); Tauri 2; Preact + TypeScript + Vitest.

**Spec:** design desta mudança em `C:\Users\csequ\projects\perf-overlay\.notes\2026-10-22-game-detection-design.md` (a pasta `.notes/` está no `.gitignore` e não existe num worktree, por isso use o caminho absoluto), sobre a spec principal `docs/superpowers/specs/2026-10-22-perf-overlay-design.md` (seções 4.3, 4.6 e 4.7). Tela: `docs/design-handoff/README.md`. As decisões abaixo valem sobre a nota.

## Decisões confirmadas com o usuário (2026-10-23)

1. Os caminhos B (pasta de biblioteca) e C (lista "É jogo") também disparam o "mostrar sozinho", igual à tela cheia.
2. Bibliotecas: Steam `\steamapps\common\`; Epic, pelos manifestos `.item` com fallback `\epic games\`; GOG, pelo registro com fallback `\gog galaxy\games\`; Xbox `\xboxgames\`; EA `\ea games\`; Ubisoft `\ubisoft game launcher\games\`. **Battle.net fica de fora.**
3. As listas do usuário guardam o **caminho completo** do exe, normalizado.
4. `general.hideOutsideGame` vem **ligado** por padrão. Chip: "Overlay ativo · aguardando jogo".
5. Layout: a linha de status continua **uma linha só**, **sem o texto do atalho** (ele já está no campo "Atalho"); o gap do painel cai de 14 para **12 px**, para o 4º switch caber sem rolagem.

Decisões deste plano (o construtor decidia): não há botão "redefinir regras", porque cada lista se desfaz pelo link oposto e o JSON é editável; as raízes de biblioteca são lidas uma vez em `Controller::start` (não de novo a cada candidato); a lista embutida de ignorados vence a lista "É jogo"; um caminho que aparece nas duas listas (JSON editado à mão) fica em `games`; a nomeação por "marcar como jogo" não depende do "mostrar sozinho".

**Fato do código que molda o desenho:** o hook de foco usa `WINEVENT_SKIPOWNPROCESS` (`crates/engine/src/native/mod.rs`), então a janela de configuração do Peek **nunca** chega a `on_foreground`. Abrir a configuração por cima do jogo mantém o jogo como primeiro plano para o detector. O "último app" é o último evento de foco que não está na lista embutida de ignorados.

## Global Constraints

- Só Windows 11 x64; rustc 1.96 (msvc), Node 24 e npm. Rode os comandos na raiz do repositório (ou do worktree).
- Sem polling (spec 4.4): tudo continua por evento. Manifestos do Epic e registro do GOG são lidos uma vez, em `Controller::start`.
- Não-objetivos: ler módulos/memória do processo do jogo, injetar, hookar, consultar a rede, manter base de jogos própria.
- Anti-objetivo: jogo em tela cheia ou janela sem bordas (The Witcher 3) é detectado e aparece sozinho **exatamente como hoje**.
- Textos exatos (pt-BR): "Esconder fora do jogo", "Overlay ativo · aguardando jogo", "Overlay ativo", "Overlay oculto", "Jogo detectado", "não é jogo", "Nenhum jogo · último app", "marcar como jogo", "Nenhum jogo detectado".
- `settings.json`: `general.hideOutsideGame` (padrão `true`) e `"gameRules": { "games": [], "notGames": [] }`, com caminhos completos normalizados (minúsculas, `\`). Um caminho nunca fica nas duas listas.
- Contrato página ↔ host: comandos `mark_not_game` e `mark_game`, sem argumentos, devolvendo `Settings` (ou erro em texto). `Status` ganha `lastAppName: string | null` e `overlayWaiting: boolean`. `overlayVisible` passa a ser a **intenção** (overlay ligado, pelo usuário ou pelo "mostrar sozinho"), não o que está desenhado.
- Ao fim de cada tarefa, `cargo test -p overlay-engine` (120 testes verdes hoje) e `npm test` (6 hoje) passam. Os testes de `controller` e `native` criam janelas reais: rode numa sessão de desktop.
- Commits em inglês, Conventional Commits, terminando com a linha `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Caminho do exe ilegível** (processo protegido, anti-cheat, `OpenProcess` negado): sem caminho, nada de lista nem de biblioteca, só o nome. Tela cheia (caminho A) continua valendo, e "não é jogo" sem caminho devolve erro sem mexer nas listas. → `classify_without_a_path_uses_only_the_name` (Task 3), `marks_without_a_target_change_nothing` (Task 6).
2. **`settings.json` editado à mão** com maiúsculas, `/`, espaços, duplicados ou o mesmo caminho nas duas listas: tudo é normalizado ao carregar, e o caminho que está nas duas fica em `games`. → `game_rules_are_normalized_on_load` (Task 2).
3. **Raiz de biblioteca que é prefixo de outra pasta** (`d:\jogos\foo` × `d:\jogos\foobar\x.exe`), raiz com `\` no fim ou raiz que é só o drive: nenhum falso positivo. → `library_roots_match_whole_folders_only` (Task 1).
4. **Manifesto do Epic quebrado, pasta ausente ou chave do GOG ausente:** são ignorados em silêncio e o app sobe normalmente. → `epic_roots_skip_broken_manifests` e `load_reads_this_machine_without_failing` (Task 1).
5. **"Marcar como jogo" num app cuja janela já fechou:** o caminho entra na lista, mas o app não vira candidato, para o ETW não rodar 10 s à toa. → `mark_game_on_a_closed_window_only_lists_it` (Task 6).

---

## Estrutura de arquivos

| Arquivo | Responsabilidade |
|---|---|
| `crates/engine/src/game_rules.rs` (novo) | `normalize_path`, `LibraryRoots` (+ leitura do Epic e do GOG), `is_library_path`, `AppClass`, `classify` |
| `crates/engine/src/lib.rs` | `pub mod game_rules;` |
| `crates/engine/src/settings.rs` | `GameRules` (listas, normalização, marcação), `General::hide_outside_game` |
| `crates/engine/src/process_info.rs` | lista embutida de ignorados ampliada |
| `crates/engine/src/game.rs` | regras novas do `GameDetector`, intenção × desenhado, nomeação |
| `crates/engine/src/native/foreground.rs` | `ForegroundInfo::exe_path` |
| `crates/engine/src/model.rs` | `Status::overlay_waiting`, `Status::last_app_name` |
| `crates/engine/src/controller.rs` | classificação, raízes, último app, `mark_game` / `mark_not_game` |
| `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` | comandos `mark_not_game` / `mark_game` |
| `src/types.ts`, `src/ipc.ts`, `src/mock.ts` | espelho do contrato e backend simulado |
| `src/status.ts` + `src/status.test.ts` (novos) | `chipLabel` |
| `src/components/StatusRow.tsx`, `GeneralToggles.tsx`, `src/App.tsx`, `src/theme.css` | tela |
| spec principal, `README.md`, `ROADMAP.md`, `docs/measurements.md` | documentação e validação |

---

### Task 1: Caminhos e pastas de biblioteca (`game_rules.rs`)

**Files:**
- Create: `crates/engine/src/game_rules.rs`
- Modify: `crates/engine/src/lib.rs` (adicionar `pub mod game_rules;` em ordem alfabética)

**Interfaces:**
- Consumes: nada.
- Produces:
  - `pub fn normalize_path(path: &str) -> String`: tira espaços das pontas, troca `/` por `\` e aplica `to_lowercase()`.
  - `pub const LIBRARY_MARKERS: [&str; 6] = [r"\steamapps\common\", r"\epic games\", r"\gog galaxy\games\", r"\xboxgames\", r"\ea games\", r"\ubisoft game launcher\games\"];`
  - `#[derive(Clone, Debug, Default, PartialEq)] pub struct LibraryRoots { roots: Vec<String> }`
    - `pub fn new(roots: impl IntoIterator<Item = String>) -> LibraryRoots`: normaliza, tira o `\` final, descarta raízes vazias ou só com o drive (`c:`), remove duplicadas.
    - `pub fn roots(&self) -> &[String]`
    - `pub fn load() -> LibraryRoots`: lê o Epic e o GOG, nunca falha.
  - `pub fn is_library_path(path: &str, roots: &LibraryRoots) -> bool`: `path` já vem normalizado. É verdadeiro se contém algum marcador, ou se começa com uma raiz seguida de `\`.
  - `pub fn epic_install_location(manifest: &str) -> Option<String>`: o campo `InstallLocation` não vazio de um manifesto `.item` (JSON).
  - privadas: `fn epic_roots(manifests_dir: &Path) -> Vec<String>` e `fn gog_roots() -> Vec<String>`.

- [ ] **Step 1: Escrever os testes que falham** (módulo `tests`, mais o `win32_tests` no fim, como em `process_info.rs`)

```rust
#[test]
fn normalize_path_lowercases_and_uses_backslashes() {
    assert_eq!(normalize_path("  D:/SteamLibrary/steamapps/common/Hades/Hades.exe "),
               r"d:\steamlibrary\steamapps\common\hades\hades.exe");
}
#[test]
fn library_markers_cover_every_store() {
    let none = LibraryRoots::default();
    for p in [
        r"D:\SteamLibrary\steamapps\common\Hades\Hades.exe",
        r"C:\Program Files (x86)\Steam\steamapps\common\Celeste\Celeste.exe",
        r"C:\Program Files\Epic Games\Fortnite\FortniteGame\Binaries\Win64\FortniteClient-Win64-Shipping.exe",
        r"C:\Program Files (x86)\GOG Galaxy\Games\Cyberpunk 2077\bin\x64\Cyberpunk2077.exe",
        r"C:\XboxGames\Starfield\Content\Starfield.exe",
        r"C:\Program Files\EA Games\Battlefield 2042\BF2042.exe",
        r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games\Far Cry 6\bin\FarCry6.exe",
    ] {
        assert!(is_library_path(&normalize_path(p), &none), "{p}");
    }
    for p in [
        r"C:\Program Files\WindowsApps\Microsoft.WindowsTerminal_1.21\WindowsTerminal.exe",
        r"C:\Users\me\AppData\Local\Discord\app-1.0\Discord.exe",
        r"C:\Tools\steamapps\notes.exe",
    ] {
        assert!(!is_library_path(&normalize_path(p), &none), "{p}");
    }
}
#[test]
fn library_roots_match_whole_folders_only() {
    let roots = LibraryRoots::new([r"D:\Jogos\Foo\".to_string(), r"C:\".to_string(), String::new()]);
    assert_eq!(roots.roots(), [r"d:\jogos\foo".to_string()]);
    assert!(is_library_path(r"d:\jogos\foo\bin\foo.exe", &roots));
    assert!(!is_library_path(r"d:\jogos\foobar\foobar.exe", &roots));
    assert!(!is_library_path(r"c:\tools\x.exe", &roots));
}
#[test]
fn epic_manifest_gives_its_install_location() {
    let m = r#"{"FormatVersion":0,"DisplayName":"Hades","InstallLocation":"D:\\Jogos\\Hades","LaunchExecutable":"Hades.exe"}"#;
    assert_eq!(epic_install_location(m).as_deref(), Some(r"D:\Jogos\Hades"));
    assert_eq!(epic_install_location("{not json"), None);
    assert_eq!(epic_install_location(r#"{"InstallLocation":""}"#), None);
    assert_eq!(epic_install_location(r#"{"DisplayName":"x"}"#), None);
}
#[test]
fn epic_roots_skip_broken_manifests() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.item"), r#"{"InstallLocation":"D:\\Jogos\\Hades"}"#).unwrap();
    std::fs::write(dir.path().join("b.item"), "{not json").unwrap();
    std::fs::write(dir.path().join("c.txt"), r#"{"InstallLocation":"D:\\Outro"}"#).unwrap();
    assert_eq!(epic_roots(dir.path()), vec![r"D:\Jogos\Hades".to_string()]);
    assert!(epic_roots(&dir.path().join("missing")).is_empty());
}

// mod win32_tests
#[test]
fn load_reads_this_machine_without_failing() {
    let roots = LibraryRoots::load();
    assert!(roots.roots().iter().all(|r| *r == normalize_path(r) && !r.ends_with('\\')), "{roots:?}");
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p overlay-engine game_rules`
Expected: FAIL na compilação (`normalize_path`, `LibraryRoots` etc. não existem).

- [ ] **Step 3: Implementar em `crates/engine/src/game_rules.rs`**

Doc do módulo: "Game rules (spec 4.6): exe paths, game library folders and the classification of the foreground app."
- `load()`: `epic_roots(%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests)` (sem `ProgramData`, pula) mais `gog_roots()`, passando tudo por `LibraryRoots::new`. Termina com `crate::trace!("game_rules: {} library roots", n)`.
- `epic_roots`: só arquivos com extensão `.item` (sem diferenciar maiúsculas). Pasta ilegível, arquivo ilegível ou sem `InstallLocation` são pulados.
- `gog_roots`: abre `HKLM\SOFTWARE\WOW6432Node\GOG.com\Games` com `KEY_READ`, enumera as subchaves (`RegEnumKeyExW`) e lê o `REG_SZ` `path` de cada uma (`RegGetValueW` com a subchave e `RRF_RT_REG_SZ`). Fecha a chave no fim. Qualquer erro devolve o que já foi lido. O padrão de leitura de `REG_SZ` está em `crates/engine/src/pawnio/mod.rs` (`reg_path`).

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test -p overlay-engine game_rules`
Expected: PASS (6 testes).

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/game_rules.rs crates/engine/src/lib.rs
git commit -m "feat(engine): recognize game library folders"
```

---

### Task 2: Configurações — `gameRules` e `hideOutsideGame`

**Files:**
- Modify: `crates/engine/src/settings.rs`

**Interfaces:**
- Consumes: `game_rules::normalize_path` (Task 1).
- Produces:
  - `#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)] #[serde(rename_all = "camelCase", default)] pub struct GameRules { pub games: Vec<String>, pub not_games: Vec<String> }`
    - `pub fn normalized(self) -> GameRules`: normaliza cada item, descarta vazios e duplicados (mantém a primeira ocorrência) e tira de `not_games` o que estiver em `games`.
    - `pub fn mark_game(&mut self, path: &str)` / `pub fn mark_not_game(&mut self, path: &str)`: normaliza, tira da outra lista e acrescenta no fim, se ainda não estiver.
  - `General::hide_outside_game: bool` (JSON `hideOutsideGame`, padrão `true`).
  - `Settings::game_rules: GameRules` (JSON `gameRules`). `Settings::normalized` também normaliza as listas.

- [ ] **Step 1: Escrever os testes que falham**

Em `defaults_match_spec`: `"general"` ganha `"hideOutsideGame": true` e a raiz ganha `"gameRules": {"games": [], "notGames": []}`. Testes novos:

```rust
#[test]
fn old_settings_file_gets_the_new_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.json");
    std::fs::write(&p, r#"{"general":{"startup":false}}"#).unwrap();
    let (s, _) = load(&p);
    assert!(!s.general.startup && s.general.hide_outside_game);
    assert_eq!(s.game_rules, GameRules::default());
}
#[test]
fn game_rules_are_normalized_on_load() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("settings.json");
    std::fs::write(&p, r#"{"gameRules":{
        "games":[" C:/Games/Foo/Foo.exe","c:\\games\\foo\\foo.exe",""],
        "notGames":["C:\\Games\\Foo\\FOO.exe","D:/Bar/bar.exe"]}}"#).unwrap();
    let (s, o) = load(&p);
    assert_eq!(o, LoadOutcome::Loaded);
    assert_eq!(s.game_rules.games, vec![r"c:\games\foo\foo.exe".to_string()]);
    assert_eq!(s.game_rules.not_games, vec![r"d:\bar\bar.exe".to_string()]);
}
#[test]
fn marking_moves_a_path_between_the_lists() {
    let mut r = GameRules::default();
    r.mark_not_game(r"C:\Games\Foo\Foo.exe");
    r.mark_not_game(r"c:\games\foo\foo.exe");
    assert_eq!(r, GameRules { games: vec![], not_games: vec![r"c:\games\foo\foo.exe".into()] });
    r.mark_game("C:/Games/Foo/Foo.exe");
    assert_eq!(r, GameRules { games: vec![r"c:\games\foo\foo.exe".into()], not_games: vec![] });
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p overlay-engine settings`
Expected: FAIL na compilação (`GameRules` e `hide_outside_game` não existem).

- [ ] **Step 3: Implementar `GameRules`, o campo `hide_outside_game` e a normalização em `crates/engine/src/settings.rs`**

- [ ] **Step 4: Rodar a suíte inteira**

Run: `cargo test -p overlay-engine`
Expected: PASS (nada fora de `settings.rs` monta `General` literal).

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/settings.rs
git commit -m "feat(settings): add game rules and the hide-outside-game option"
```

---

### Task 3: Lista de ignorados e classificação do app em primeiro plano

**Files:**
- Modify: `crates/engine/src/process_info.rs`
- Modify: `crates/engine/src/game_rules.rs`

**Interfaces:**
- Consumes: `GameRules` (Task 2); `LibraryRoots`, `is_library_path` (Task 1); `process_info::is_ignored`.
- Produces:
  - `process_info::is_ignored(exe_name: &str, self_exe: &str) -> bool`: mesma assinatura; `IGNORED` vira `&[&str]` (sem tamanho fixo).
  - `#[derive(Clone, Copy, Debug, Default, PartialEq)] pub struct AppClass { pub ignored: bool, pub known_game: bool }`
  - `pub fn classify(exe_name: &str, exe_path: Option<&str>, self_exe: &str, rules: &GameRules, roots: &LibraryRoots) -> AppClass`. `exe_path` vem normalizado. `ignored` = `is_ignored(exe_name, self_exe)` ou o caminho está em `not_games`. `known_game` = não ignorado e o caminho está em `games` ou é de biblioteca. A lista embutida vence a lista "É jogo".

Entradas novas em `IGNORED` (minúsculas; a comparação já ignora maiúsculas): `windowsterminal.exe`, `openconsole.exe`, `conhost.exe`, `cmd.exe`, `powershell.exe`, `pwsh.exe`, `discord.exe`, `slack.exe`, `ms-teams.exe`, `teams.exe`, `code.exe`, `cursor.exe`, `zed.exe`, `spotify.exe`, `steam.exe`, `steamwebhelper.exe`, `epicgameslauncher.exe`, `epicwebhelper.exe`, `eadesktop.exe`, `battle.net.exe`, `galaxyclient.exe`, `ubisoftconnect.exe`, `upc.exe`, `obs64.exe`, `nvidia app.exe`, `nvidia overlay.exe`, `overwolf.exe`, `claude.exe`, `taskmgr.exe`, `systemsettings.exe`, `wallpaper32.exe`, `wallpaper64.exe`. Comente o motivo dos três menos óbvios: `epicwebhelper.exe` fica sob `\epic games\`, o Wallpaper Engine fica sob `\steamapps\common\` (os dois cairiam num marcador de biblioteca), e `upc.exe` é a janela do Ubisoft Connect.

- [ ] **Step 1: Escrever os testes que falham**

Em `process_info::tests::ignores_shell_browsers_and_self`, acrescente ao laço `"WindowsTerminal.exe"`, `"OpenConsole.exe"`, `"Discord.exe"`, `"Code.exe"`, `"steamwebhelper.exe"`, `"EpicWebHelper.exe"`, `"NVIDIA Overlay.exe"` e `"wallpaper64.exe"`. Em `game_rules::tests`:

```rust
fn rules() -> GameRules {
    GameRules {
        games: vec![r"c:\tools\emu\emu.exe".into()],
        not_games: vec![r"d:\steamlibrary\steamapps\common\tool\tool.exe".into()],
    }
}
#[test]
fn classify_by_name_lists_and_library() {
    let roots = LibraryRoots::default();
    let c = |name: &str, path: &str| classify(name, Some(path), "peek.exe", &rules(), &roots);
    assert_eq!(c("hades.exe", r"d:\steamlibrary\steamapps\common\hades\hades.exe"), AppClass { ignored: false, known_game: true });
    assert_eq!(c("emu.exe", r"c:\tools\emu\emu.exe"), AppClass { ignored: false, known_game: true });
    assert_eq!(c("tool.exe", r"d:\steamlibrary\steamapps\common\tool\tool.exe"), AppClass { ignored: true, known_game: false });
    assert_eq!(c("notepad.exe", r"c:\windows\notepad.exe"), AppClass::default());
    assert_eq!(c("Peek.exe", r"c:\program files\peek\peek.exe"), AppClass { ignored: true, known_game: false });
}
#[test]
fn builtin_ignore_wins_over_the_games_list() {
    let discord = r"c:\users\me\appdata\local\discord\discord.exe";
    let r = GameRules { games: vec![discord.into()], ..GameRules::default() };
    assert_eq!(classify("Discord.exe", Some(discord), "peek.exe", &r, &LibraryRoots::default()),
               AppClass { ignored: true, known_game: false });
}
#[test]
fn classify_without_a_path_uses_only_the_name() {
    let roots = LibraryRoots::new([r"d:\jogos".to_string()]);
    assert_eq!(classify("witcher3.exe", None, "peek.exe", &rules(), &roots), AppClass::default());
    assert!(classify("cmd.exe", None, "peek.exe", &rules(), &roots).ignored);
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p overlay-engine game_rules process_info`
Expected: FAIL (`classify`/`AppClass` não existem; os nomes novos não são ignorados).

- [ ] **Step 3: Implementar `IGNORED` ampliado e `classify`**

- [ ] **Step 4: Rodar e ver passar**

Run: `cargo test -p overlay-engine`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/process_info.rs crates/engine/src/game_rules.rs
git commit -m "feat(engine): classify the foreground app by ignore list, user rules and library"
```

---

### Task 4: `GameDetector` — tela cheia ou jogo conhecido, intenção × desenhado, nomeação

**Files:**
- Modify: `crates/engine/src/game.rs`
- Modify: `crates/engine/src/controller.rs` (só para compilar: `set_visible` vira `set_on`; o literal `Fg` ganha `known_game: false`, que a Task 5 preenche)

**Interfaces:**
- Consumes: nada.
- Produces:
  - `pub struct Fg { pub pid: u32, pub hwnd: isize, pub covers_monitor: bool, pub ignored: bool, pub known_game: bool }`
  - `pub struct Game { pub pid: u32, pub hwnd: isize }`: `DetectPath` e `Game::path` saem (nada fora de `game.rs` os usa).
  - `GameDetector::new(auto_show: bool)`: começa com `hide_outside_game = false`.
  - `pub fn set_hide_outside_game(&mut self, on: bool)`
  - `pub fn set_on(&mut self, on: bool)`: o antigo `set_visible`, com a mesma lógica sobre a intenção. `toggle()` inverte `is_on()`.
  - `pub fn is_on(&self) -> bool`: `mode != Hidden` (ligado pelo usuário ou pelo "mostrar sozinho").
  - `pub fn visible(&self) -> bool`: o que está **desenhado**. `Auto` → `true`; `Manual` → `!hide_outside_game || game_in_front`; `Hidden` → `false`. `game_in_front` = o `fg` é o processo do jogo detectado.
  - `pub fn waiting(&self) -> bool`: `is_on() && !visible()`.
  - `pub fn nominate(&mut self, pid: u32, hwnd: isize)`

Regras (substituem os dois caminhos da spec 4.6):
- **Candidato pelo foco:** `!fg.ignored`, não é o jogo detectado, não foi desistido, `fg.covers_monitor || fg.known_game`, e `auto_show || is_on()`. O caminho "qualquer janela com o overlay visível" deixa de existir.
- **Nomeado:** vira candidato quando o foco não dá nenhum. Não depende de `auto_show` nem de `is_on()`. Nunca é o jogo detectado. Sai quando é confirmado, quando é desistido (senão o próximo `on_foreground`, que limpa `given_up`, o reviveria) e quando seu processo termina (`on_process_exited` também o descarta, mesmo sem ser o jogo). Um `nominate` novo substitui o anterior.
- **Modo `Auto`** (ao confirmar ou ao focar): `auto_show`, modo `Hidden`, sem supressão **e o jogo em primeiro plano**, ou seja, `on_presenting` passa a exigir `fg.pid == pid`. Um nomeado confirmado em segundo plano espera o foco. Os três caminhos (A, B e C) mostram sozinhos.

- [ ] **Step 1: Ajustar e escrever os testes**

Apague `windowed_process_is_candidate_only_while_visible` e `manual_show_over_fullscreen_uses_fullscreen_path`. O helper `fg()` ganha `known_game: false`. Acrescente:

```rust
fn kg(pid: u32) -> Fg { Fg { known_game: true, ..fg(pid, false) } }
fn peek() -> Fg { Fg { ignored: true, ..fg(7, false) } }

#[test]
fn windowed_app_is_never_candidate_even_with_the_overlay_on() {
    let mut d = GameDetector::new(true);
    d.on_foreground(fg(3, false));
    d.toggle();
    assert_eq!(d.candidate_pid(), None);
    d.on_presenting(3, CONFIRM_MS, 0);
    assert!(d.game().is_none());
}
#[test]
fn known_game_in_a_window_is_confirmed_and_auto_shown() {
    let mut d = GameDetector::new(true);
    d.on_foreground(kg(3));
    assert_eq!(d.candidate_pid(), Some(3));
    d.on_presenting(3, CONFIRM_MS, 0);
    assert_eq!(gpid(&d), Some(3));
    assert!(d.visible());
    d.on_foreground(fg(9, false));
    assert!(!d.visible());
    d.on_foreground(kg(3));
    assert!(d.visible());
}
#[test]
fn known_game_in_a_window_needs_auto_show_or_the_overlay() {
    let mut d = GameDetector::new(false);
    d.on_foreground(kg(3));
    assert_eq!(d.candidate_pid(), None);
    d.toggle();
    assert_eq!(d.candidate_pid(), Some(3));
}
#[test]
fn manual_overlay_over_fullscreen_with_auto_show_off_confirms_the_game() {
    let mut d = GameDetector::new(false);
    d.on_foreground(fg(4, true));
    d.toggle();
    d.on_presenting(4, CONFIRM_MS, 0);
    assert_eq!(gpid(&d), Some(4));
}
#[test]
fn hide_outside_game_draws_the_manual_overlay_only_over_the_game() {
    let mut d = GameDetector::new(false);
    d.set_hide_outside_game(true);
    d.toggle();
    assert_eq!((d.is_on(), d.visible(), d.waiting()), (true, false, true));
    d.on_foreground(fg(1, true));
    d.on_presenting(1, CONFIRM_MS, 0);
    assert_eq!((d.visible(), d.waiting()), (true, false));
    d.on_foreground(fg(9, false));
    assert_eq!((d.is_on(), d.visible(), d.waiting()), (true, false, true), "Alt+Tab hides the manual overlay too");
    d.on_foreground(fg(1, true));
    assert!(d.visible());
    d.on_process_exited(1);
    assert_eq!((d.is_on(), d.waiting()), (true, true));
}
#[test]
fn turning_hide_outside_game_off_shows_the_waiting_overlay() {
    let mut d = GameDetector::new(false);
    d.set_hide_outside_game(true);
    d.toggle();
    d.set_hide_outside_game(false);
    assert_eq!((d.visible(), d.waiting()), (true, false));
}
#[test]
fn toggle_while_waiting_turns_the_overlay_off() {
    let mut d = GameDetector::new(false);
    d.set_hide_outside_game(true);
    d.toggle();
    d.toggle();
    assert_eq!((d.is_on(), d.waiting()), (false, false));
}
#[test]
fn waiting_overlay_runs_nothing() {
    use crate::effects::{Effects, plan_effects};
    let mut d = GameDetector::new(false);
    d.set_hide_outside_game(true);
    d.toggle();
    d.on_foreground(fg(9, false));
    let m = crate::settings::Metrics::default();
    assert_eq!(plan_effects(d.visible(), false, d.candidate_pid().is_some(), &m), Effects::default());
}
#[test]
fn nominee_is_confirmed_in_the_background_and_shown_on_focus() {
    let mut d = GameDetector::new(true);
    d.on_foreground(peek());
    d.nominate(3, 30);
    assert_eq!(d.candidate_pid(), Some(3));
    d.on_presenting(3, CONFIRM_MS, 0);
    assert_eq!(gpid(&d), Some(3));
    assert_eq!(d.candidate_pid(), None);
    assert!(!d.visible(), "not in front yet");
    d.on_foreground(kg(3));
    assert!(d.visible());
}
#[test]
fn given_up_nominee_is_not_revived_by_focus_changes() {
    let mut d = GameDetector::new(true);
    d.nominate(3, 30);
    d.on_presenting(3, 0, 0);
    d.on_presenting(3, 0, GIVE_UP_MS);
    assert_eq!(d.candidate_pid(), None);
    d.on_foreground(peek());
    assert_eq!(d.candidate_pid(), None);
}
#[test]
fn nominee_that_exits_is_dropped() {
    let mut d = GameDetector::new(true);
    d.nominate(3, 30);
    d.on_process_exited(3);
    assert_eq!(d.candidate_pid(), None);
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p overlay-engine game::`
Expected: FAIL na compilação (`known_game`, `set_hide_outside_game`, `nominate`, `is_on`, `waiting` não existem).

- [ ] **Step 3: Implementar as regras em `crates/engine/src/game.rs` e os dois ajustes de compilação em `controller.rs`**

Atualize o doc de `on_presenting` e o comentário de `refresh_candidate` com as regras novas. O status do controller continua usando `visible()` nesta tarefa: com `hide_outside_game` desligado, ele é igual a `is_on()`.

- [ ] **Step 4: Rodar a suíte inteira**

Run: `cargo test -p overlay-engine`
Expected: PASS. Todos os testes antigos de `game.rs` e `controller.rs` que sobraram passam sem mudança.

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/game.rs crates/engine/src/controller.rs
git commit -m "feat(engine): detect only fullscreen or known games, hide outside the game and accept nominations"
```

---

### Task 5: Controller — classificação, raízes de biblioteca e "Esconder fora do jogo"

**Files:**
- Modify: `crates/engine/src/native/foreground.rs`
- Modify: `crates/engine/src/model.rs`
- Modify: `crates/engine/src/controller.rs`

**Interfaces:**
- Consumes: `game_rules::{normalize_path, classify, AppClass, LibraryRoots}` (Tasks 1 e 3); `GameDetector::{set_hide_outside_game, is_on, visible, waiting}` (Task 4); `General::hide_outside_game` (Task 2).
- Produces:
  - `ForegroundInfo::exe_path: Option<PathBuf>`: o caminho completo da imagem. `describe` chama `process_info::exe_path` uma vez e tira o `exe_name` dele.
  - `Status::overlay_waiting: bool` (JSON `overlayWaiting`). `Status::overlay_visible` passa a vir de `detector.is_on()`; atualize o doc do campo: "the overlay is on (hotkey, chip, tray or auto-show); with `overlay_waiting` it is on but hidden until the detected game is in front".
  - `Inner::libraries: LibraryRoots`: padrão vazio; `start()` faz `LibraryRoots::load()` e guarda **antes** de `NativeHandle::spawn`, para o primeiro evento de foco já ver as raízes.
  - privado `fn class_of(&self, g: &Inner, info: &ForegroundInfo) -> AppClass`: normaliza `info.exe_path` e chama `classify` com `g.settings.game_rules` e `g.libraries`.

Mudanças no controller:
- `on_foreground` monta o `Fg` com `class_of` (`ignored` e `known_game`).
- `location_target` usa `!self.class_of(g, fg).ignored` no lugar de `process_info::is_ignored`.
- `with_self_exe` e `update_settings` repassam `general.hide_outside_game` com `detector.set_hide_outside_game`.
- `sync`, passo 7: `Status { overlay_visible: is_on(), game_name, overlay_waiting: waiting() }`. Mostrar/âncora (passo 6) e `plan()` continuam com `visible()` (desenhado), então "aguardando" pausa o ETW e os sensores.

- [ ] **Step 1: Adaptar os helpers e escrever os testes**

Helpers de teste em `controller::tests`:
- `focus` monta `ForegroundInfo { exe_path: std::env::current_exe().ok(), .. }`. O novo `focus_as(c, hwnd, pid, exe_path: &str, covers_monitor: bool)` faz o mesmo com o caminho dado. Os literais `other`/notepad ganham `exe_path: None`.
- `fn me() -> String`: `normalize_path` de `std::env::current_exe()`, o caminho que `focus` informa e que `exe_path(pid)` lê deste processo.
- `fn shown(log: &Log) -> Vec<serde_json::Value>`: projeta cada `"status"` em `{"overlayVisible", "gameName"}` e descarta repetições seguidas, partindo de `{"overlayVisible": false, "gameName": null}`. Assim, campos novos de `Status` não mexem nos testes. `detect_game`, `fullscreen_presenter_is_detected_named_and_lost_with_its_window`, `recreated_game_window_keeps_the_game_and_its_suppression` (o `len() == 2` vira `shown(&log).len() == 2`) e `watched_game_is_not_ended_by_a_dead_window` passam a comparar `shown(&log)`.
- `toggling_without_native_thread_emits_status_once_per_change`: com `hideOutsideGame` ligado por padrão, os payloads esperados são `{"overlayVisible": true, "gameName": null, "overlayWaiting": true}` e depois `{"overlayVisible": false, "gameName": null, "overlayWaiting": false}`.
- `model::tests::app_state_serializes_for_the_ui`: o literal ganha `overlay_waiting: false`; acrescente `assert_eq!(v["status"]["overlayWaiting"], false);`.

Testes novos:

```rust
#[test]
fn windowed_app_with_the_overlay_on_is_not_detected() {
    let (c, _log, _dir) = controller(Settings::default());
    let pid = std::process::id();
    c.toggle_overlay();
    let hwnd = top_level_window(windows::core::w!("Jogo Teste"));
    focus(&c, hwnd, pid, false);
    assert_eq!(c.inner().detector.candidate_pid(), None);
    present_and_tick(&c, pid);
    assert_eq!(c.inner().detector.game(), None);
    destroy(hwnd);
}
#[test]
fn steam_game_in_a_window_is_detected_and_auto_shown() {
    let (c, log, _dir) = controller(Settings::default());
    let pid = std::process::id();
    let hwnd = top_level_window(windows::core::w!("Jogo Teste"));
    focus_as(&c, hwnd, pid, r"D:\SteamLibrary\steamapps\common\Hades\Hades.exe", false);
    present_and_tick(&c, pid);
    assert_eq!(shown(&log), vec![serde_json::json!({"overlayVisible": true, "gameName": "Jogo Teste"})]);
    destroy(hwnd);
}
#[test]
fn not_games_list_blocks_a_fullscreen_window() {
    let rules = crate::settings::GameRules { not_games: vec![me()], ..Default::default() };
    let (c, _log, _dir) = controller(Settings { game_rules: rules, ..Settings::default() });
    let hwnd = top_level_window(windows::core::w!("Jogo Teste"));
    focus(&c, hwnd, std::process::id(), true);
    assert_eq!(c.inner().detector.candidate_pid(), None);
    destroy(hwnd);
}
#[test]
fn hide_outside_game_waits_and_pauses_outside_the_game() {
    let (c, log, _dir) = controller(Settings::default());
    let pid = std::process::id();
    let last = || log.payloads("status").last().cloned().unwrap();
    c.toggle_overlay();
    assert_eq!((last()["overlayVisible"].clone(), last()["overlayWaiting"].clone()), (true.into(), true.into()));
    let hwnd = top_level_window(windows::core::w!("Jogo Teste"));
    focus(&c, hwnd, pid, true);
    present_and_tick(&c, pid);
    assert_eq!(last()["overlayWaiting"], false);
    let other = ForegroundInfo { hwnd: 1, pid: 4242, exe_name: "notepad.exe".into(), exe_path: None, title: String::new(), covers_monitor: false };
    c.on_native_event(NativeEvent::Foreground(other));
    assert_eq!(last()["overlayWaiting"], true);
    assert_eq!(c.inner().plan(), Effects::default(), "hidden outside the game: nothing runs");
    c.update_settings(serde_json::json!({"general": {"hideOutsideGame": false}})).unwrap();
    assert_eq!(last()["overlayWaiting"], false);
    assert!(c.inner().detector.visible());
    destroy(hwnd);
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p overlay-engine controller model`
Expected: FAIL na compilação (`exe_path`, `overlay_waiting`, `focus_as` não existem).

- [ ] **Step 3: Implementar `exe_path` em `describe`, `overlay_waiting`, `libraries`, `class_of` e as mudanças listadas no controller**

- [ ] **Step 4: Rodar a suíte inteira**

Run: `cargo test -p overlay-engine`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/native/foreground.rs crates/engine/src/model.rs crates/engine/src/controller.rs
git commit -m "feat(engine): wire game rules and hide-outside-game into the controller"
```

---

### Task 6: Último app, "não é jogo" e "marcar como jogo"

**Files:**
- Modify: `crates/engine/src/model.rs`
- Modify: `crates/engine/src/controller.rs`

**Interfaces:**
- Consumes: `GameRules::{mark_game, mark_not_game}` (Task 2); `GameDetector::{nominate, on_process_exited}` (Task 4); `normalize_path` (Task 1); `foreground::is_window_alive`.
- Produces:
  - `Status::last_app_name: Option<String>` (JSON `lastAppName`).
  - `pub fn mark_not_game(&self) -> Result<Settings, String>` e `pub fn mark_game(&self) -> Result<Settings, String>` no `Controller`.
  - `Inner::last_app: Option<LastApp>`, com `struct LastApp { pid: u32, hwnd: isize, path: Option<String> /* normalized */, name: String }`, e `Inner::game_path: Option<String>` (caminho normalizado do jogo detectado).

Comportamento:
- **Último app:** o último primeiro plano cujo exe não está na lista **embutida** (`process_info::is_ignored`, que já cobre o próprio app). Apps em `notGames` contam, para poderem ser remarcados. O nome é `app_name(&info)` = `display_name(file_description(exe_path), título, exe_name)`, calculado **antes** de pegar `inner` (lê o arquivo).
- **`retarget`** chama `process_info::exe_path(pid)` uma vez e usa o resultado para o nome (`fn game_name(game: Game, exe_path: Option<&Path>, last_fg: Option<&ForegroundInfo>) -> String`) e para `game_path`.
- **`mark_not_game`:** sem jogo → `Err("Nenhum jogo detectado.")`; sem `game_path` → `Err("Caminho do jogo indisponível.")`. Caso contrário: monta `next` (clone das configurações + `mark_not_game(path)`), salva (se falhar, devolve `Err` e nada muda), adota `next`, chama `detector.on_process_exited(pid)` ("como se tivesse fechado"), faz `sync` e emite `"settings"`.
- **`mark_game`:** sem último app ou sem caminho → `Err("Nenhum app para marcar.")`. Caso contrário: `mark_game(path)`, salva, adota; se `foreground::is_window_alive(hwnd)`, chama `detector.nominate(pid, hwnd)`; `sync`; emite `"settings"`.
- `sync`, passo 7: `last_app_name: g.last_app.as_ref().map(|a| a.name.clone())`.

- [ ] **Step 1: Escrever os testes que falham**

`model::tests::app_state_serializes_for_the_ui`: o literal ganha `last_app_name: None`; acrescente `assert!(v["status"]["lastAppName"].is_null());`. `toggling_without_native_thread_emits_status_once_per_change`: os dois payloads esperados ganham `"lastAppName": null`. Em `controller::tests` (`me()` vem da Task 5):

```rust
#[test]
fn last_app_skips_this_app_and_ignored_ones() {
    let dir = tempfile::tempdir().unwrap();
    let log = Arc::new(Log::default());
    let c = Controller::with_self_exe(Settings::default(), dir.path().join("settings.json"), log.clone(), "peek.exe".into());
    let last = || log.payloads("status").last().cloned().unwrap()["lastAppName"].clone();
    let hwnd = top_level_window(windows::core::w!("Jogo Teste"));
    focus(&c, hwnd, std::process::id(), false);
    assert_eq!(last(), "Jogo Teste");
    for exe in ["Peek.exe", "explorer.exe", "WindowsTerminal.exe"] {
        let info = ForegroundInfo { hwnd: 1, pid: 4242, exe_name: exe.into(), exe_path: None, title: "x".into(), covers_monitor: false };
        c.on_native_event(NativeEvent::Foreground(info));
    }
    assert_eq!(last(), "Jogo Teste");
    destroy(hwnd);
}
#[test]
fn mark_not_game_lists_the_game_ends_it_and_persists() {
    let (c, log, dir) = controller(Settings::default());
    let hwnd = detect_game(&c, &log);
    let saved = c.mark_not_game().unwrap();
    assert_eq!(saved.game_rules.not_games, vec![me()]);
    assert_eq!(crate::settings::load(&dir.path().join("settings.json")).0.game_rules.not_games, vec![me()]);
    assert_eq!(shown(&log).last(), Some(&serde_json::json!({"overlayVisible": false, "gameName": null})));
    assert_eq!(log.payloads("settings").len(), 1);
    focus(&c, hwnd, std::process::id(), true);
    assert_eq!(c.inner().detector.candidate_pid(), None, "listed: never a candidate again");
    destroy(hwnd);
}
#[test]
fn mark_game_nominates_the_last_app_and_unlists_it() {
    let rules = crate::settings::GameRules { not_games: vec![me()], ..Default::default() };
    let (c, log, _dir) = controller(Settings { game_rules: rules, ..Settings::default() });
    let pid = std::process::id();
    let hwnd = top_level_window(windows::core::w!("Jogo Teste"));
    focus(&c, hwnd, pid, false);
    // The tray click puts the shell in front; the settings window itself is never reported.
    let shell = ForegroundInfo { hwnd: 1, pid: 4242, exe_name: "explorer.exe".into(), exe_path: None, title: String::new(), covers_monitor: false };
    c.on_native_event(NativeEvent::Foreground(shell));
    let saved = c.mark_game().unwrap();
    assert_eq!((saved.game_rules.games, saved.game_rules.not_games), (vec![me()], vec![]));
    assert_eq!(c.inner().detector.candidate_pid(), Some(pid));
    present_and_tick(&c, pid);
    assert_eq!(shown(&log).last(), Some(&serde_json::json!({"overlayVisible": false, "gameName": "Jogo Teste"})),
               "detected; it shows once it is in front");
    destroy(hwnd);
}
#[test]
fn mark_game_on_a_closed_window_only_lists_it() {
    let (c, _log, _dir) = controller(Settings::default());
    let hwnd = top_level_window(windows::core::w!("Jogo Teste"));
    focus(&c, hwnd, std::process::id(), false);
    destroy(hwnd);
    assert_eq!(c.mark_game().unwrap().game_rules.games, vec![me()]);
    assert_eq!(c.inner().detector.candidate_pid(), None);
}
#[test]
fn marks_without_a_target_change_nothing() {
    let (c, log, dir) = controller(Settings::default());
    assert!(c.mark_game().is_err());
    assert!(c.mark_not_game().is_err());
    let hwnd = detect_game(&c, &log);
    c.inner().game_path = None; // a game whose image path can't be read
    assert!(c.mark_not_game().is_err());
    assert_eq!(c.state().settings.game_rules, crate::settings::GameRules::default());
    assert!(log.payloads("settings").is_empty());
    assert!(!dir.path().join("settings.json").exists());
    destroy(hwnd);
}
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `cargo test -p overlay-engine controller model`
Expected: FAIL na compilação (`mark_game`, `mark_not_game`, `last_app_name`, `game_path` não existem).

- [ ] **Step 3: Implementar o último app, `game_path`, `mark_not_game`, `mark_game` e `last_app_name`**

- [ ] **Step 4: Rodar a suíte inteira**

Run: `cargo test -p overlay-engine`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/engine/src/model.rs crates/engine/src/controller.rs
git commit -m "feat(engine): mark the last app as a game or the detected game as not one"
```

---

### Task 7: Casca Tauri e tela de configuração

**Files:**
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`
- Modify: `src/types.ts`, `src/ipc.ts`, `src/mock.ts`, `src/App.tsx`, `src/components/StatusRow.tsx`, `src/components/GeneralToggles.tsx`, `src/theme.css`
- Create: `src/status.ts`, `src/status.test.ts`

**Interfaces:**
- Consumes: `Controller::mark_not_game` / `mark_game` (Task 6); JSON de `Status` `{overlayVisible, gameName, lastAppName, overlayWaiting}`; JSON de `Settings` com `general.hideOutsideGame` e `gameRules`.
- Produces:
  - Comandos Tauri `mark_not_game(ctl) -> Result<Settings, String>` e `mark_game(ctl) -> Result<Settings, String>`, registrados no `generate_handler!`.
  - `Api.markNotGame(): Promise<Settings>` e `Api.markGame(): Promise<Settings>` (`invoke('mark_not_game')` / `invoke('mark_game')`).
  - `export function chipLabel(s: Status): string` em `src/status.ts`.
  - Em `types.ts`: `Settings.general.hideOutsideGame: boolean`, `Settings.gameRules: { games: string[]; notGames: string[] }`, `Status.lastAppName: string | null`, `Status.overlayWaiting: boolean`.

- [ ] **Step 1: Escrever o teste que falha** (`src/status.test.ts`)

```ts
import { expect, it } from 'vitest';
import { chipLabel } from './status';

const off = { overlayVisible: false, overlayWaiting: false, gameName: null, lastAppName: null };

it('chip label follows the overlay state', () => {
  expect(chipLabel(off)).toBe('Overlay oculto');
  expect(chipLabel({ ...off, overlayVisible: true })).toBe('Overlay ativo');
  expect(chipLabel({ ...off, overlayVisible: true, overlayWaiting: true })).toBe('Overlay ativo · aguardando jogo');
});
```

- [ ] **Step 2: Rodar e ver falhar**

Run: `npm test`
Expected: FAIL (`./status` não existe).

- [ ] **Step 3: Implementar `types.ts`, `ipc.ts`, `status.ts`, os comandos e o registro**

- [ ] **Step 4: Rodar e ver passar**

Run: `npm test`
Expected: PASS (7 testes).

- [ ] **Step 5: Atualizar o mock (`src/mock.ts`)**

`DEFAULT_SETTINGS` ganha `hideOutsideGame: true` e `gameRules: { games: [], notGames: [] }`. O status inicial ganha `lastAppName: 'Hollow Knight'` e `overlayWaiting: false`. Novo estado de URL `?waiting=1`: sem jogo e aguardando (acrescente ao comentário do topo). O mock finge que o jogo detectado está em primeiro plano, então `setOverlayVisible` e `updateSettings` recalculam `overlayWaiting = overlayVisible && settings.general.hideOutsideGame && gameName == null` e emitem `status` se mudou. `markNotGame`: sem jogo → rejeita; senão acrescenta um caminho falso em `notGames` (tirando de `games`), `lastAppName = gameName`, `gameName = null`, recalcula, emite `settings` e `status` e devolve as configurações. `markGame`: sem `lastAppName` → rejeita; senão acrescenta em `games` (tirando de `notGames`), emite `settings`, devolve, e depois de 2000 ms faz `gameName = lastAppName`, recalcula e emite `status`.

- [ ] **Step 6: Tela**

`StatusRow` recebe `{ status, onToggle, onMarkNotGame, onMarkGame }` (sai `shortcut` e o `<span class="status-shortcut">`). O chip mostra `chipLabel(status)`, e a bolinha fica `on` quando `overlayVisible`. Bloco do jogo:

```tsx
{status.gameName != null ? (
  <div class="status-game">
    <span class="status-game-label">Jogo detectado</span>
    <span class="status-game-line">
      <span class="status-game-name">{status.gameName}</span>
      <span class="status-sep">·</span>
      <button class="status-link" onClick={onMarkNotGame}>não é jogo</button>
    </span>
  </div>
) : status.lastAppName != null ? (
  <div class="status-game">
    <span class="status-game-label">Nenhum jogo · último app</span>
    <span class="status-game-line">
      <span class="status-game-name">{status.lastAppName}</span>
      <span class="status-sep">·</span>
      <button class="status-link" onClick={onMarkGame}>marcar como jogo</button>
    </span>
  </div>
) : (
  <span class="status-nogame">Nenhum jogo detectado</span>
)}
```

- `App.tsx`: deixa de passar `shortcut` ao `StatusRow` e passa `onMarkNotGame={() => api.markNotGame().catch((e) => console.error('mark_not_game failed:', e))}` (o mesmo para `markGame`). Os eventos `settings` e `status` atualizam a tela.
- `GeneralToggles`: 4ª linha `['hideOutsideGame', 'Esconder fora do jogo']`.
- `theme.css`: `.panel { gap: 12px }` (comente: "12, not the handoff's 14, so the 4th Geral switch fits without scrolling"); `.status-game { flex: 1; min-width: 0 }`; `.status-game-line { display: flex; align-items: baseline; gap: 5px; min-width: 0 }`; `.status-game-name` perde o `max-width: 190px` e ganha `min-width: 0` (as reticências vêm do flex); `.status-sep { font-size: 11px; color: var(--t55) }`; `.status-link { flex: none; padding: 0; border: 0; background: none; font: inherit; font-size: 11px; color: var(--t55); cursor: pointer; white-space: nowrap }`; `.status-link:hover { color: #fff; text-decoration: underline }`; remova `.status-shortcut`.

- [ ] **Step 7: Verificar o build**

Run: `npm test && npm run build && cargo check -p perf-overlay`
Expected: 7 testes passam, `tsc` sem erros, build do Vite ok e o crate Tauri compila.

- [ ] **Step 8: [VISUAL — coordenador] Conferir a tela no navegador**

`npm run dev` e abrir, num viewport de 480×680, `http://localhost:1420/`, `/?nogame=1` e `/?waiting=1`. Tire um print de cada. Conferir: estado padrão sem barra de rolagem; nome longo com reticências antes do link; link claro no hover; clicar em "não é jogo" e depois em "marcar como jogo" alterna os blocos; o 4º switch liga e desliga.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src/
git commit -m "feat(ui): add the hide-outside-game switch and game marking links"
```

---

### Task 8: Documentação e validação manual

**Files:**
- Modify: `docs/superpowers/specs/2026-10-22-perf-overlay-design.md` (4.3, 4.4, 4.6, 4.7)
- Modify: `README.md`, `ROADMAP.md`, `docs/measurements.md`

**Interfaces:**
- Consumes: tudo das Tasks 1 a 7.
- Produces: spec, README e ROADMAP atualizados; resultados da validação registrados.

- [ ] **Step 1: Atualizar a spec principal**

- **4.6**, reescrita a partir da nota: caminhos A (tela cheia), B (biblioteca) e C ("É jogo"), todos com "mostrar sozinho"; lista embutida de ignorados (aponte para `process_info.rs`) e listas do usuário por caminho; bibliotecas e de onde vêm; "não é jogo" e "marcar como jogo" (último app e nomeação); "Esconder fora do jogo" com o texto do chip; o caminho "qualquer janela com o overlay visível" foi removido.
- **4.7**: o JSON de exemplo ganha `"hideOutsideGame": true` em `general` e `"gameRules": { "games": [], "notGames": [] }`.
- **4.3**, em "Diferenças entre o protótipo e o app real": a linha de status perde o texto do atalho e ganha os links; o chip ganha o estado "aguardando jogo"; o gap do painel cai para 12 px.
- **4.4**: "com o overlay visível" passa a significar desenhado (aguardando jogo não conta).

- [ ] **Step 2: README e ROADMAP**

No README, troque o item "Game detection is a heuristic" por um que explique a regra nova (tela cheia/sem bordas, pasta de biblioteca ou marcado pelo usuário; os links "não é jogo" / "marcar como jogo"; as listas ficam em `gameRules` no `settings.json`) e cite a opção "Esconder fora do jogo". No ROADMAP, remova o item "Game detected in a window never upgrades to the fullscreen path", que deixa de existir.

- [ ] **Step 3: Commit da documentação**

```bash
git add docs/superpowers/specs/2026-10-22-perf-overlay-design.md README.md ROADMAP.md
git commit -m "docs: describe the new game detection rules and hide-outside-game"
```

- [ ] **Step 4: [USUÁRIO][ADMIN] Validação manual com o build release instalado**

Mesmo fluxo da validação de 2026-10-22 (`docs/measurements.md`). Conferir:
1. The Witcher 3 em janela sem bordas aparece sozinho, como antes.
2. Windows Terminal com o overlay ligado pelo atalho não é detectado.
3. Um jogo Steam em janela comum é detectado e aparece sozinho.
4. "não é jogo" e "marcar como jogo" funcionam e persistem depois de reiniciar o Peek.
5. Com "Esconder fora do jogo", o overlay ligado pelo atalho some no desktop (chip "Overlay ativo · aguardando jogo") e volta no jogo.
6. `scripts/measure.ps1 -Name Peek` oculto e aguardando jogo: < 20 MB e ~0% de CPU.

- [ ] **Step 5: Registrar os resultados e fazer o commit**

Acrescente a `docs/measurements.md` uma seção "Detecção de jogo (data)" com os 6 itens e as medições.

```bash
git add docs/measurements.md
git commit -m "docs: record the game detection validation"
```
