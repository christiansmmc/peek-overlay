# Handoff: Tela de configuração do Overlay de desempenho

## Overview
Tela de configuração do app que fica na bandeja do Windows. Ela abre quando o usuário clica no ícone da bandeja. Com o atalho (padrão Ctrl+Shift+O), o app mostra por cima do jogo um overlay com FPS, gráfico de frametime, uso e temperatura da GPU, e uso e temperatura da CPU. Tudo fica numa tela só, sem menus, e cada mudança vale na hora (não existe botão Salvar). Idioma: pt-BR.

## About the Design Files
`Configuracoes Overlay.dc.html` é uma **referência de design feita em HTML**: um protótipo que mostra o visual e o comportamento esperados. Não é código de produção. Abra no Chrome/Edge (precisa do `support.js` na mesma pasta). A tarefa é **recriar essa tela** no app real: uma janela nativa com WebView2, usando HTML/CSS/JS puro ou React leve, 100% offline. O arquivo .dc.html usa um runtime próprio, então não reaproveite o `support.js`. Toda a lógica está na classe `Component` no fim do arquivo, e os estilos estão inline no template.

## Fidelity
**Alta fidelidade.** Cores, tipografia, espaçamentos, estados e textos são finais. Recrie pixel a pixel.

## Restrições da janela
- 480×680 px fixos, sem redimensionar, sem borda nativa. Cantos de 8px (o DWM do Win11 arredonda).
- Barra de título desenhada pela página: área de arrastar com `-webkit-app-region: drag` (os botões usam `no-drag`). No WebView2, habilite o suporte a regiões não cliente (`IsNonClientRegionSupportEnabled`) ou implemente o arraste via host.
- Minimizar → minimiza. Fechar → **só esconde** a janela (o app continua na bandeja). No protótipo isso é simulado com uma tela preta e o botão "reabrir".
- Escala de 100% a 200%: use px em CSS; o WebView2 aplica o DPI automaticamente.
- Sem fontes, imagens ou scripts externos. Fonte: `'Segoe UI Variable Text','Segoe UI Variable','Segoe UI',system-ui`. Overlay: `'Segoe UI Variable Display'`.
- Sem seleção de texto (`user-select:none`).

## Layout (de cima para baixo)
Fundo da janela `#0a0a12`, liso (sem manchas de cor; a versão aprovada é essa).

1. **Barra de título**, altura 40. À esquerda (padding 16): quadrado 12×12 com raio 4 na cor `P` e anel de 3px em `P/25%`, seguido do texto "Overlay de desempenho" (12px, branco 82%). À direita: botões Minimizar e Fechar de 46×40, ícones de linha de 10px e 1px de espessura. Hover: fundo branco 8%; no Fechar, `#c42b1c` com ícone branco.
2. **Painel de vidro**: `top:40 left:12 right:12 bottom:12`, raio 16, padding `14 16 16`, `background: rgba(255,255,255,.055)`, `backdrop-filter: blur(28px) saturate(140%)`, borda 1px branco 10%, sombra `inset 0 1px 0 rgba(255,255,255,.08), 0 20px 50px rgba(0,0,0,.35)`. Coluna flex com gap 14.
   - **Status**, altura mínima 28: chip clicável (altura 28, raio 14, fundo branco 7%, borda branco 10%) com bolinha de 7px (ativo `oklch(0.78 0.15 150)` com anel de 22%; oculto, branco 40%) e o texto "Overlay ativo" ou "Overlay oculto" (12,5px, 600). Clicar alterna o estado. Em seguida: "Jogo detectado" (10,5px, branco 55%) com o nome do jogo embaixo (12,5px, 600, reticências a partir de 190px), ou "Nenhum jogo detectado". À direita: o atalho atual como texto ("Ctrl + Shift + O", 11px, branco 55%).
   - **Prévia**: o rótulo de seção "Prévia · clique para posicionar" fica à esquerda e o nome da posição à direita (11,5px, branco 75%), com o sufixo " · prévia reduzida" quando o overlay é encolhido para caber. A caixa tem altura 158, raio 10, borda branco 8%. O fundo é um placeholder listrado (`repeating-linear-gradient(135deg, branco 2,5% 0 10px, transparente 10–20px)` sobre `linear-gradient(#1b1e2c,#10121a)`) com o texto central "cena do jogo · 1920×1080" em monospace de 10px. Há **8 alvos de posição** (área de 26×26, bolinha de 8px; a selecionada usa `P`, as outras branco 12%, todas com borda branco 45%; hover com scale 1.35), afastados 14px das bordas: os 4 cantos e o meio de cada borda. O overlay fica com 8px de margem da borda, `pointer-events:none`, escala = tamanho × fit (o fit encolhe para caber em largura−16 e altura−16). Com o overlay oculto, a opacidade cai para 0.3.
   - **Métricas**: o rótulo seguido de pílulas em flex-wrap com gap 8. Cada pílula: altura 32, raio 16, padding `0 12 0 9`, 12,5px peso 500, indicador redondo de 14px. Ligada: fundo `P/20%`, borda `P/60%`, indicador preenchido em `P` com check escuro. Desligada: fundo branco 4%, borda branco 14%, texto branco 80%, indicador em anel de branco 40%. Labels: "FPS", "Gráfico de frametime", "GPU uso", "GPU temp.", "CPU uso", "CPU temp.".
   - **Linha de 3 colunas** (`124px 1fr 1fr`, gap 18): **Estilo** é um controle segmentado (fundo preto 28%, borda branco 7%, raio 10, padding 3; botões com altura 28 e raio 7; o ativo tem fundo branco 15% e sombra `0 1px 2px rgba(0,0,0,.3)`, o inativo texto branco 65%) com as opções "Linha" e "Card". **Tamanho** (75–200%, passo 5) e **Opacidade** (fundo do overlay, 30–100%, passo 5) são sliders próprios: trilho de 4px em branco 14%, preenchimento em `P`, puxador branco de 16px com sombra, área de toque com altura 32. O valor aparece à direita do rótulo (12px, 600, números tabulares). Aceitam arrastar e as setas do teclado.
   - **Atalho**: à esquerda, "Atalho mostrar/esconder" (13px, 500) com a dica embaixo (11,5px). À direita, um campo de no mínimo 156×36, raio 9, fundo preto 28%, mostrando as teclas como keycaps (padding 3×7, raio 5, fundo branco 11%, 11,5px, 600).
   - **Geral**: 3 linhas de 30px; a linha inteira é clicável. Switch de 36×20: ligado com fundo `P` e knob escuro `#0b0b12`; desligado com fundo branco 8%, borda branco 25% e knob branco 75%. Transições de .15s. Textos: "Iniciar com o Windows", "Abrir minimizado na bandeja", "Mostrar overlay sozinho ao abrir um jogo".
   - Rótulos de seção: 11px, 600, `letter-spacing .06em`, caixa alta, branco 55%.

## Prévia = overlay real (Direct2D)
A prévia precisa usar **só** o que o Direct2D consegue desenhar: retângulo arredondado com cor sólida e alpha, texto com sombra simples (`0 1px 2px rgba(0,0,0,.85)`), linha (polyline) e barras. Nada de blur, gradiente ou animação além dos números mudando. Números com `tabular-nums` e decimal com vírgula ("6,9 ms").
- **Linha**: pílula com altura 26, raio 13, padding 0 12, gap 12, 12px peso 600. Itens: `142` + "FPS" (9,5px, branco 65%) · gráfico 44×14 · "GPU" (`oklch(0.78 0.13 290)`) "96% 71°" · "CPU" (`oklch(0.78 0.13 250)`) "38% 64°".
- **Card**: largura 148, padding `9 11 10`, raio 10, coluna com gap 6. FPS em 26px/700 (tracking −.02em) + "FPS" 10px + "6,9 ms" à direita · gráfico 126×26 · linhas GPU/CPU em grid `26px 1fr auto` com barra de 5px (trilho branco 14%, preenchimento GPU `oklch(0.72 0.15 290)`, CPU `oklch(0.72 0.15 250)`) e o valor "96% · 71°".
- Fundo do overlay: `rgba(12,12,18, opacidade)`.
- Linha do gráfico: `oklch(0.78 0.13 250)`, 1,4 a 1,5px, eixo Y de 0 a 14 ms.
- Se só a temperatura estiver ligada, a barra fica invisível mas mantém o espaço. Sem nenhuma métrica: "Nenhuma métrica ativa".
- No protótipo, os valores de exemplo mudam a cada 500ms (com picos ocasionais de FPS).

## Estados
- **Temperatura da CPU sem driver**: a pílula fica desligada com o sub-rótulo "requer driver" (11px, 600, `oklch(0.8 0.12 75)`). Ao clicar, abre uma folha inferior (scrim `rgba(6,6,12,.55)` abaixo da barra de título; cartão com raio 16, padding 18, fundo `rgba(30,28,46,.9)` com blur de 30). Título: "Temperatura da CPU precisa de um driver". Texto explicando o PawnIO (gratuito, assinado, pede permissão de administrador). Botões "Agora não" (secundário) e "Instalar" (fundo `P`, texto `#0b0b12`, 700). Clicar no scrim fecha.
- **Instalando…**: spinner de 16px, "Instalando PawnIO…", a dica "Se o Windows pedir permissão, clique em “Sim”…" e uma barra de progresso de 4px. Não dá para fechar.
- **Falhou**: título em `oklch(0.8 0.12 25)` "Não foi possível instalar o driver", explicação, e os botões "Agora não" / "Tentar de novo". A pílula passa a mostrar "instalação falhou" (na cor de erro).
- **Sucesso**: a folha fecha, a métrica é ligada e aparece o toast "Driver instalado. Temperatura da CPU ativada." (2,6s; centralizado embaixo; fundo `rgba(24,24,36,.95)`, raio 10).
- **Métrica indisponível no hardware** (ex.: temperatura de GPU em placa integrada): pílula com borda tracejada, fundo transparente, texto branco 50%, sub-rótulo "sem sensor", `cursor:not-allowed` e tooltip com o motivo. Não responde a clique.
- **Capturando atalho**: clicar no campo inicia a captura. A borda fica `P` com anel de 3px em `P/25%`, aparece "Pressione as teclas…" e os modificadores entram ao vivo seguidos de "+ …". A dica mostra "Esc cancela". Esc sem modificador cancela; perder o foco cancela. Uma tecla sem modificador mostra o erro "Use pelo menos Ctrl, Alt ou Shift junto com a tecla." e continua capturando.
- **Atalho em uso**: "X já é usado por <App>. Escolha outra." aparece na dica, na cor de erro, com a borda do campo em `oklch(0.72 0.15 25/.7)`, e o atalho anterior é mantido. No app real, detecte o conflito com `RegisterHotKey` (falha = em uso). A lista do protótipo é só um exemplo.
- Sucesso ao trocar o atalho: toast "Atalho alterado para Ctrl + Alt + P".

## State
`metrics{fps,graph,gpuUse,gpuTemp,cpuUse,cpuTemp}`, `style: 'line'|'card'`, `pos: tl|tc|tr|ml|mr|bl|bc|br`, `size` (75–200), `opacity` (30–100), `shortcut: string[]`, `capturing`, `scError`, `general{startup,minimized,autoShow}`, `overlayActive`, `gameName|null`, `driver: missing|installing|failed|installed`, disponibilidade de hardware por métrica. Valores padrão: todas as métricas ligadas, exceto a temperatura da CPU (requer driver); Card; canto superior direito; 100%; 80%; Ctrl+Shift+O; as três opções gerais ligadas.
Persista cada mudança imediatamente e envie para o host nativo (ex.: `chrome.webview.postMessage`). O host manda de volta status, jogo detectado, disponibilidade de sensores e o resultado da instalação.

## Design tokens
- `P` (acento): `oklch(0.72 0.15 290)`; variante clara `oklch(0.78 0.13 290)`
- Azul (CPU/gráfico): `oklch(0.72 0.15 250)` / `oklch(0.78 0.13 250)`
- Aviso: `oklch(0.8 0.12 75)` · Erro: `oklch(0.8 0.12 25)` · Ativo: `oklch(0.78 0.15 150)`
- Fundo da janela `#0a0a12`; texto `#fff` com alphas de 55/65/75/80%
- Raios: 5 (keycap), 7/8/9/10, 13/16 (pílulas), 16 (painel/folha)
- Fonte 11–15px; números com `font-variant-numeric: tabular-nums`

## Assets
Nenhum. Os ícones (minimizar, fechar, check) são SVGs simples de linha, inline.

## Files
- `Configuracoes Overlay.dc.html`: protótipo (versão aprovada, fundo liso)
- `support.js`: runtime necessário só para abrir o protótipo no navegador
