# Overlay de desempenho: design

**Data:** 2026-10-22
**Status:** aprovado (2026-10-22)
**Nota (nome):** o app recebeu o nome **Peek**. Onde esta spec fala do nome visível, o título da janela de configuração, a dica da bandeja, o nome do executável/instalador e a pasta de configurações (`%APPDATA%\Peek`) usam "Peek". Nomes internos (crates `overlay-engine` e `perf-overlay`, sessão ETW) não mudaram.

## 1. Intenção

Um app pessoal pra Windows que fica rodando em segundo plano e, com um atalho, mostra por cima do jogo um overlay pequeno com FPS, gráfico de frametime, uso e temperatura da GPU, e uso e temperatura da CPU.

**Por quê:** jogando The Witcher 3, o desempenho caiu e o usuário quis ver na hora se o gargalo era GPU ou CPU. As opções que existem não servem:

- MSI Afterburner + RivaTuner: dá trabalho configurar e deixar bonito.
- Xbox Game Bar: o usuário não gosta.
- Overlay da Steam: o contador de FPS dela fica ligado o tempo todo e **continua assim**. Este app é a "olhada mais funda", que aparece quando o usuário aperta o atalho.

A referência de funcionalidade é o overlay do [ResetExplode](https://github.com/GustavoReset/ResetExplode): janela transparente, sempre no topo, que ignora cliques, com FPS lido via ETW sem injetar nada no jogo. Daquele projeto só interessa o overlay.

## 2. Objetivos, não-objetivos e anti-objetivos

**Objetivos**
- Atalho global (padrão Ctrl+Shift+O) que mostra e esconde o overlay.
- Seis métricas, cada uma pode ser ligada e desligada: FPS, gráfico de frametime, GPU uso, GPU temperatura, CPU uso, CPU temperatura.
- Dois estilos de overlay, "Linha" e "Card", e 8 posições na tela.
- Uma tela de configuração moderna, que já é o design final (seção 4.3).
- Rodar em segundo plano quase sem custo (orçamento na seção 6).

**Não-objetivos**
- Monitor de hardware completo: nada de dezenas de métricas, histórico, logs ou gráficos fora do overlay.
- Benchmarks, stress test e as outras funções do ResetExplode.
- Substituir o contador de FPS da Steam.
- Distribuição pública. É uso pessoal: sem atualização automática e sem telemetria.

**Anti-objetivos** (o app falha nesses casos, mesmo que funcione)
- **Pesar no jogo.** Um app de medir desempenho não pode derrubar desempenho.
- **Exigir configuração pra ficar bom.** Tem que abrir com a cara certa e os padrões certos.
- **Mexer no processo do jogo.** Nada de injeção, hook de DirectX ou leitura de memória. Isso protege contra anti-cheat.
- **Overlay roubar foco ou cliques** do jogo.

## 3. Restrições

- **Plataforma:** Windows 11 (64 bits). A máquina do usuário é um desktop com AMD Ryzen 7 5800X e NVIDIA RTX 4070 Ti. Rust (cargo) e winget já estão instalados.
- **Roda elevado:** o manifesto pede administrador. A sessão ETW em tempo real e o acesso ao driver PawnIO exigem.
- **Sem injeção:** FPS e frametime vêm de eventos ETW. Sensores vêm de APIs do Windows ou do driver PawnIO.
- **UI 100% offline:** nenhuma fonte, imagem ou script de rede.
- **Idioma:** português do Brasil, com decimal com vírgula ("6,9 ms").

## 4. Comportamento

### 4.1 Ciclo de vida

- **Instância única.** Abrir o executável de novo com o app já rodando abre a tela de configuração da instância existente.
- **Bandeja:** ícone com menu nativo do Windows: "Configurações", "Mostrar/esconder overlay" e "Sair". Clicar no ícone abre a configuração.
- **Ao iniciar:** com "Abrir minimizado na bandeja" ligado, o app só aparece na bandeja. Desligado, abre a configuração. Na **primeira execução** (sem arquivo de configurações), a configuração abre sempre, pra o usuário ver que o app existe (decisão minha). O overlay começa sempre oculto (decisão minha) e só aparece pelo atalho ou pela detecção de jogo (4.6).
- **Iniciar com o Windows:** como o app roda elevado, a chave `Run` do registro não serve: o Windows bloqueia ou pede UAC a cada boot. Use uma tarefa do Agendador de Tarefas, no logon do usuário, com "executar com privilégios mais altos". O padrão vem **ligado**, então a tarefa é criada na primeira execução. Desligar a opção remove a tarefa.
- **Atalho global** via `RegisterHotKey`: alterna o overlay entre visível e oculto, faz o mesmo que o chip de status da configuração e funciona com qualquer janela em foco.
- **Sair** (pelo menu da bandeja) encerra tudo: sessão ETW, threads e janelas.

### 4.2 Overlay no jogo

- **Janela:** sempre no topo, transparente, sem foco e invisível pra cliques (`WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE`). Não aparece no Alt+Tab nem na barra de tarefas.
- **Monitor:** o overlay fica no monitor da janela do jogo detectado (4.6). Sem jogo detectado, fica no monitor da janela em primeiro plano (decisão minha). A cada troca de janela em primeiro plano (`SetWinEventHook(EVENT_SYSTEM_FOREGROUND)`), o app reafirma o "sempre no topo".
- **Posição:** os 4 cantos e o meio de cada borda (`tl tc tr ml mr bl bc br`), com uma margem pequena da borda (≈12 px a 100% de DPI; valor exato fica pro construtor). Sem posição central, pra não cobrir a mira.
- **Tamanho:** de 75% a 200%, multiplicado pela escala de DPI do monitor.
- **Opacidade:** de 30% a 100%, e vale só pro fundo (`rgba(12,12,18,α)`). O texto fica sempre opaco.
- **Visual:** o desenho dos estilos Linha e Card, as cores e as tipografias estão em `docs/design-handoff/README.md`, seção "Prévia = overlay real". A prévia da tela de configuração e o overlay real precisam ficar **iguais**.
- **Só primitivas simples:** retângulo arredondado com cor sólida e alpha, texto com sombra simples, polyline e barras. Sem blur, gradiente, imagem ou animação.
- **Renderização sem a placa de vídeo** (decisão minha): o overlay é uma imagem de poucas centenas de pixels, então deve ser desenhado na CPU. Sugestão: Direct2D com render target de software ou DC, mais DirectWrite pro texto, e `UpdateLayeredWindow` com alpha por pixel. Assim o app evita criar um dispositivo D3D, que carrega o driver da GPU (dezenas de MB) e disputa a GPU com o jogo. Se o construtor provar que outra abordagem fica dentro do orçamento da seção 6, pode usá-la.
- **Atualização:** os números mudam a cada 500 ms, como no protótipo. O gráfico de frametime pode redesenhar com mais frequência (≈10–15 Hz) pra mostrar os picos com fluidez.
- **Sem dado:** métrica ligada sem dado (por exemplo, FPS sem jogo detectado) mostra "—" no lugar do número.
- **Sem métrica:** se nenhuma métrica estiver ligada, o overlay mostra "Nenhuma métrica ativa", como no handoff.

### 4.3 Tela de configuração

**O design é final e está em `docs/design-handoff/`.** O `README.md` traz o layout, os estados, os tokens e os textos, e `Configuracoes Overlay.dc.html` é o protótipo navegável (abre no Edge ou no Chrome, com o `support.js` na mesma pasta). A fidelidade é alta: a tela deve ser recriada pixel a pixel. O protótipo usa um runtime próprio, então **não** reaproveite o `support.js`.

Diferenças entre o protótipo e o app real:

- **Janela:** 480×680 fixa, sem borda nativa, com barra de título desenhada pela página (`-webkit-app-region: drag`). Minimizar minimiza.
- **Fechar destrói a janela e o WebView** (decisão minha), em vez de só esconder. Pro usuário o efeito é o mesmo, porque o app continua na bandeja, e isso libera a memória do WebView2 (~100 MB). Reabrir leva cerca de 1 segundo, e isso é aceito.
- **Valores ao vivo:** a prévia mostra os valores reais quando há dados. Sem jogo, mostra valores de exemplo animados, como no protótipo.
- **Conflito de atalho:** `RegisterHotKey` só diz que o atalho está em uso, não por quem. A mensagem real é "X já é usado por outro app. Escolha outra.". Se a combinação estiver na lista conhecida do protótipo (Discord, Steam etc.), a mensagem cita o nome. Atalhos interceptados por hook, como o Shift+Tab da Steam, não falham no `RegisterHotKey`, então a lista conhecida também serve de proteção pra esses. Combinações da lista são **bloqueadas** como no protótipo, mantendo o atalho anterior.
- **Barra de tarefas:** a janela de configuração tem botão na barra de tarefas enquanto está aberta (decisão minha).
- **Durante a captura do atalho**, o host desregistra o atalho atual. Senão o próprio atalho é consumido pelo sistema e nunca chega na página. Ao fim da captura, registra o novo; se cancelar, re-registra o antigo.
- **Barra de progresso da instalação do driver:** pode ser indeterminada, porque o winget não reporta progresso de forma confiável.
- **Chip de status:** clicar nele alterna o overlay de verdade.
- **Jogo detectado:** mostra o nome do jogo atual (4.6).

### 4.4 Métricas: de onde vêm

| Métrica | Fonte | Regras |
|---|---|---|
| FPS | ETW em tempo real: eventos `Present` do provider Microsoft-Windows-DXGI (e D3D9, se for barato), filtrados pelo PID do jogo | Quadros na janela de 500 ms ÷ tempo. **DX11/DX12 é obrigatório** (The Witcher 3). Vulkan/OpenGL é desejável: se der pra cobrir via eventos do DxgKrnl sem complicar muito, cubra; senão, esses jogos mostram "—" e isso é aceito. |
| Frametime (gráfico e "6,9 ms" do Card) | Mesmos eventos: intervalo entre Presents consecutivos do processo (o "MsBetweenPresents" do PresentMon) | O número é a média da janela de 500 ms. O gráfico mostra os últimos ~3 s de quadros, reduzidos à largura do gráfico **preservando o máximo de cada coluna**, pra que as travadas apareçam. **Eixo Y com escala automática** (decisão minha): o handoff fixa de 0 a 14 ms, mas esse valor foi tirado do exemplo de 142 FPS e cortaria um jogo a 60 FPS (16,7 ms). |
| GPU uso | PDH: `\GPU Engine(*engtype_3D)\Utilization Percentage`, somado por adaptador | Use o adaptador (LUID) que o processo do jogo usa; o nome da instância PDH traz o pid e o luid. Sem jogo, use o adaptador dedicado. Deve bater com o Gerenciador de Tarefas. |
| GPU temp. | `D3DKMTQueryAdapterInfo` com `KMTQAITYPE_ADAPTERPERFDATA` (campo `Temperature`, em décimos de °C), no mesmo adaptador | É de onde o Gerenciador de Tarefas tira. Funciona sem criar dispositivo D3D. Valor 0 ou erro significa "sem sensor" (estado do handoff). |
| CPU uso | PDH `\Processor Information(_Total)\% Processor Utility`, limitado a 100 | Deve bater com o Gerenciador de Tarefas. |
| CPU temp. | Driver PawnIO (4.5): AMD Zen via módulo AMDFamily17 (registrador SMN de temperatura, Tctl), Intel via módulo IntelMSR (`IA32_PACKAGE_THERM_STATUS` + TjMax de `MSR_TEMPERATURE_TARGET`) | O LibreHardwareMonitor é a referência de como ler cada família. **AMD Zen é obrigatório** (máquina do usuário: Ryzen 7 5800X, sem offset de Tctl); Intel moderno também deve funcionar. CPU fora dessas famílias fica como "sem sensor". |

- **Sensores sob demanda:** a sessão ETW e a leitura de sensores só rodam com o overlay visível, com a tela de configuração aberta, ou quando a detecção de jogo precisa confirmar um candidato (4.6). Com o overlay oculto e a configuração fechada, o app fica parado, esperando eventos, sem nenhum polling.
- **Métrica desligada não é coletada.**

### 4.5 Driver PawnIO (temperatura da CPU)

- **Detecção:** ao iniciar, o app verifica se o PawnIO está instalado (o serviço/driver e a `PawnIOLib.dll` na pasta de instalação dele).
- **Instalado:** a pílula "CPU temp." funciona normalmente.
- **Não instalado:** a pílula aparece com "requer driver", e clicar nela abre a folha inferior do handoff.
- **"Instalar"** roda `winget install --id namazso.PawnIO -e --silent --accept-package-agreements --accept-source-agreements` (decisão minha: driver oficial, sempre a versão atual, sem empacotar nada). Precisa de internet nesse momento, o que é aceito.
- **Resultado:** sucesso liga a métrica e mostra o toast. Falha (sem winget, sem internet, usuário recusou o UAC, erro do instalador) leva ao estado "Falhou" do handoff, com "Tentar de novo".
- **Os módulos do PawnIO** (os `.bin` assinados) vão embutidos no executável, como o LibreHardwareMonitor faz. Vêm do repositório oficial PawnIO.Modules. Confira a licença antes de embutir.

### 4.6 Detecção de jogo e "mostrar overlay sozinho"

Esta seção inteira é decisão minha.

**Processos ignorados:** do sistema (explorer, dwm etc.), navegadores (Edge, Chrome, Firefox, Opera, Brave etc.) e o próprio app.

**Como um processo vira "o jogo detectado"** (dois caminhos):

1. **Tela cheia:** a janela em primeiro plano ocupa o monitor inteiro (tela cheia ou janela sem borda) e apresenta quadros via ETW de forma contínua por ~2 s. Esse é o único caminho que dispara o "mostrar sozinho".
2. **Com o overlay visível:** qualquer processo não ignorado em primeiro plano que apresente quadros de forma contínua por ~2 s, mesmo em janela que não ocupa a tela. Isso cobre jogos em modo janela.

**Quanto tempo dura:** o jogo continua detectado até o processo **fechar** ou outro jogo ser confirmado. Perder o foco **não** desfaz a detecção. Assim, abrir a configuração por cima do jogo continua mostrando "Jogo detectado" e valores reais na prévia. O fim do processo é detectado por evento (espera no handle do processo), sem polling.

**De onde vêm o FPS e o frametime:** sempre do jogo detectado. Sem jogo detectado, mostram "—".

**Nome exibido:** o `FileDescription` do executável; se não existir, o título da janela; se não existir, o nome do .exe.

**"Mostrar overlay sozinho" ligado** (padrão do handoff):
- O overlay aparece quando o jogo detectado (caminho 1) está em primeiro plano.
- Some quando o jogo perde o foco e volta quando ele recupera o foco (Alt+Tab ida e volta).
- Se o usuário escondeu o overlay pelo atalho, ele não reaparece sozinho até o processo do jogo fechar ou outro jogo ser detectado. Alt+Tab não conta como "mudar de jogo". O atalho manda mais que a automação.

**Overlay ligado pelo atalho:** continua visível independente do foco. Enquanto o jogo detectado estiver rodando, mostra as métricas dele.

**Quando a sessão ETW roda:** com o overlay visível, com a configuração aberta, ou enquanto um candidato está sendo confirmado. Fora isso, fica parada. Voltar o foco pro jogo é um evento de foco e não precisa de ETW.

### 4.7 Configurações salvas

JSON em `%APPDATA%\Peek\settings.json`, gravado a cada mudança (não existe botão Salvar). Formato:

```json
{
  "version": 1,
  "metrics": { "fps": true, "graph": true, "gpuUse": true, "gpuTemp": true, "cpuUse": true, "cpuTemp": false },
  "style": "card",
  "pos": "tr",
  "size": 100,
  "opacity": 80,
  "shortcut": ["Ctrl", "Shift", "O"],
  "general": { "startup": true, "minimized": true, "autoShow": true }
}
```

- **Padrões:** os do handoff, mostrados acima.
- **Arquivo ausente ou corrompido:** o app volta aos padrões e não trava.
- **`cpuTemp` ligado sem driver:** se o driver sumir, a métrica é tratada como desligada e a pílula volta a "requer driver".

## 5. Arquitetura (decidida)

- **Rust + Tauri v2.** O Tauri cuida da janela de configuração (WebView2), da bandeja, da comunicação com a página e do instalador. O resto é Rust usando o crate `windows`.
- **Frontend:** TypeScript + Vite, recriando o handoff. A escolha entre JS puro e um framework leve (Preact ou similar) fica pro construtor; o critério é ficar fiel ao handoff e ser simples. Nada de bibliotecas de componentes.
- **Sem janela inicial:** o app inicia sem nenhuma janela Tauri (só bandeja) e cria a de configuração sob demanda. Fechar destrói a janela. Fechar a última janela **não** encerra o app.
- **Overlay:** janela Win32 própria, com a sua thread e o seu loop de mensagens, fora do WebView. Desenho conforme 4.2.
- **Módulos sugeridos** (o construtor pode reorganizar):
  - `settings`: carrega e salva o JSON e notifica as mudanças.
  - `hotkey`: registro global e troca com detecção de conflito.
  - `frames`: sessão ETW, frametimes por PID (crate `ferrisetw` ou equivalente).
  - `sensors`: GPU uso, GPU temp, CPU uso, CPU temp; cada um reporta disponível, indisponível (com motivo) ou valor.
  - `pawnio`: detecção, instalação via winget e leitura de temperatura.
  - `game`: detecção de jogo (4.6).
  - `overlay`: janela e desenho.
  - `autostart`: tarefa no Agendador.
- **Contrato entre a página e o host** (comandos e eventos do Tauri; os nomes podem mudar, o conteúdo não):
  - página → host:
    - `get_state` devolve settings, status e disponibilidade;
    - `update_settings(parcial)`;
    - `set_overlay_visible(bool)`;
    - `begin_hotkey_capture` e `end_hotkey_capture`;
    - `try_set_hotkey(combo)` devolve ok ou conflito (com o app, se conhecido);
    - `install_driver`;
    - `window_minimize` e `window_close`.
  - host → página:
    - `status` (overlay visível, nome do jogo ou null);
    - `metrics` (amostra a cada 500 ms, só com a configuração aberta);
    - `availability` (por métrica: ok, sem sensor com motivo, ou requer driver);
    - `driver` (missing, installing, failed ou installed).

## 6. Orçamento de recursos

Como medir (decisão minha):
- **Ferramenta:** Gerenciador de Tarefas, aba Detalhes, coluna "Memória (conjunto de trabalho privado ativo)".
- **O que medir:** só o processo do app, sem contar os processos filhos do WebView2. Esses só podem existir com a configuração aberta e precisam sumir quando ela fecha.
- **Ambiente:** build release, na máquina do usuário.
- **Sem truque:** não vale reduzir o working set à força (`EmptyWorkingSet`, `SetProcessWorkingSetSize`) pra bater o número.

| Situação | RAM (privada, processo do app) | CPU |
|---|---|---|
| Em segundo plano, overlay oculto, configuração fechada | < 20 MB | ~0% (sem polling) |
| Overlay visível com as 6 métricas, durante um jogo | < 40 MB | < 1% |
| Configuração aberta | o que o WebView2 usar; tem que voltar ao primeiro patamar depois de fechar | n/a |

**Impacto no jogo:** ligar o overlay no The Witcher 3 não pode mudar o FPS médio de forma perceptível em relação ao overlay desligado.

**Os números são meta, não muro** (decisão minha):
- A primeira tarefa do plano é medir o mínimo: Tauri só com bandeja, sem janela, em build release.
- Se só essa base já passar do orçamento, os números se ajustam e o usuário é avisado.
- **Não** divida o app em dois processos pra ganhar alguns MB: simplicidade vale mais aqui.
- O orçamento continua valendo pro que o app adiciona em cima da base.

## 7. Limitações aceitas

- **Tela cheia exclusiva de verdade:** o overlay não aparece. No Windows 11, a maioria dos jogos DX11/DX12 roda em modo que aceita overlay. Se algum esconder, a solução é usar janela sem borda.
- **Vulkan/OpenGL:** FPS e frametime podem não aparecer (4.4).
- **Administrador:** o app pede administrador quando aberto manualmente. No logon não pede, porque usa o Agendador.
- **Driver PawnIO:** instalar exige internet e aceitar o UAC.
- **Detecção de jogo:** é heurística. Algum app em tela cheia que não é jogo pode ser detectado. Navegadores já ficam de fora.

## 8. Deixado pro construtor

- Estrutura de pastas, nomes de módulos, crates (exceto o que a seção 5 fixa) e estratégia de testes.
- Framework do frontend (JS puro ou um framework leve).
- Margem exata do overlay, frequência de redesenho do gráfico e algoritmo da escala automática do eixo Y. Os limites estão em 4.2 e 4.4.
- Como cobrir Vulkan/OpenGL, se der.
- Detalhes da lista de processos ignorados na detecção de jogo.
- Ícone da bandeja e do app: simples, coerente com o acento roxo do handoff (`oklch(0.72 0.15 290)`).
- Formato do instalador (o NSIS do Tauri basta).

## 9. Referências

- Design da tela: `docs/design-handoff/README.md` e `docs/design-handoff/Configuracoes Overlay.dc.html`
- ResetExplode (referência de comportamento do overlay e do FPS via ETW): https://github.com/GustavoReset/ResetExplode
- PresentMon (referência de como medir frames via ETW): https://github.com/GameTechDev/PresentMon
- LibreHardwareMonitor (referência de leitura de temperatura via PawnIO): https://github.com/LibreHardwareMonitor/LibreHardwareMonitor
- PawnIO: https://github.com/namazso/PawnIO e https://github.com/namazso/PawnIO.Modules
