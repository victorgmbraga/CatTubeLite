<p align="center">
  <img src="packaging/io.github.coderastic.CatTube.svg" width="128" alt="CatTubeLite">
</p>

<p align="center">
  <a href="README.md">English</a> · <strong>Português (BR)</strong>
</p>

# CatTubeLite

Um fork leve do [CatTube](https://github.com/coderastic/CatTube) — aplicativo nativo de YouTube para Linux feito para TVs e controles remotos — otimizado para TV boxes Linux ARM64 com hardware limitado (como o BTV9 com GPU Mali-450 e 1–2 GiB de RAM), mantendo compatibilidade total com desktops Linux.

Assim como o CatTube original, abre em tela cheia, funciona com setas, OK e Voltar no controle remoto ou teclado (e controles de videogame no CatosTV), não exibe anúncios e reproduz vídeos via `mpv` com SponsorBlock. Este fork tira o que depende de conta do YouTube ou do celular: login, comentários, chat ao vivo, contagem de dislikes e controle pelo celular.

## O que mudou neste fork

O CatTubeLite parte do CatTube `1.0.4` e reúne estas frentes de mudanças voltadas para desempenho, estabilidade e menor uso de CPU/GPU/RAM:

### Interface plana e renderização leve
- **Tema plano em preto puro (`FLAT`):** remove fundos animados, backdrops borrados, camadas translúcidas de vidro, sombras suaves, brilhos especulares, overdraw redundante, dithering no fragment shader (`dithering: false`) e animações de fade em toasts, evitando passagens caras em GPUs como a Mali-450.
- **Zero trabalho desperdiçado atrás do player:** durante a abertura ou enquanto o `mpv` está em tela cheia, o app para de redesenhar o catálogo e baixar miniaturas; em `low_power`, também limpa o cache de texturas OpenGL para deixar mais RAM livre em TV boxes de 1 GiB.
- **Renderer OpenGL (`glow`) por padrão em `low_power`:** usa `glow` em vez de `wgpu` em dispositivos de baixo desempenho, com fallback automático entre `glow` e `wgpu` quando `CATTUBE_RENDERER` não é definido manualmente.
- **Layout de texto e listas mais rápido:** adiciona cache limitado de medição de texto (`fit` / `two_lines` com busca binária), consulta O(1) das posições de retomada, menos segmentos de curva com movimento reduzido, culling vertical em Configurações e remoção de `stat()` por frame e clones de `Page` a cada tecla.

### Reprodução otimizada para ARM64 e dispositivos fracos
- **Perfil de baixo desempenho automático (`CATTUBE_LOW_POWER`):** ativado automaticamente com até 3 GiB de RAM, em GPUs Mali-400/450 ou VideoCore IV e em máquinas ARM com menos de 6 GiB (pode ser forçado com `CATTUBE_LOW_POWER=1` ou desativado com `0`). Novas configurações usam **720p SDR**, **H.264** e buffer de **32 MiB** em aparelhos com `<= 1,5 GiB` de RAM (**64 MiB** nos demais), preservando preferências já salvas.
- **Runtimes JS em ARM64, 720p60 em H.264 e exclusão de AV1:** baixa o binário `aarch64-unknown-linux-gnu` do Deno em ARM64, também aceita QuickJS do sistema (`qjs`/`quickjs`) ou `CATTUBE_JS_RUNTIME`, libera até **720p60** para H.264 em `low_power` mantendo `<= 30 fps` em resoluções maiores / outros codecs e exclui AV1 antes do fallback genérico quando AVC ou VP9 é preferido.
- **`mpv`, IPC orientado a eventos e OSD mais leves:** adiciona `--hwdec=auto-safe` (salvo quando `CATTUBE_HWDEC` ou `mpv.conf` escolhe o decodificador), `--profile=fast`, `--scale=bilinear`, `--cscale=bilinear`, `--dscale=bilinear`, `--dither-depth=no` e `--deband=no` no modo `low_power`; assina `eof-reached`, `duration`, `pause` e `ytdl_is_live` via `observe_property` sem polling constante no IPC; para o timer do OSD enquanto pausado (`1 s` reproduzindo em `low_power`), omite camadas de frost e desfoques gaussianos do libass (`\blur140` / `\blur70`), reduz a geometria do spinner, grava checkpoints periódicos a cada `30 s` em `low_power` (`5 s` padrão), exibe aviso amigável se o `mpv`/`yt-dlp` falhar ao iniciar um vídeo e mapeia a tecla Voltar (`GO_BACK`) no `mpv`.
- **Atlas de vídeos relacionados em memória e em lotes:** monta o painel de vídeos relacionados diretamente em memória (limite de 12 vídeos em meia escala `840×480` em `low_power`, 24 em escala cheia no padrão), baixa miniaturas `mqdefault` em paralelo e atualiza o overlay em lotes de 4 sem arquivos temporários por miniatura.

### Arquitetura de memória, rede e E/S
- **Caches de imagem limitados em RAM e em disco:** limita texturas em memória (`16 MiB` em `low_power`, `64 MiB` padrão, despejo LRU com prioridade ao foco e lado máximo de `320 px` / `640 px`) e limpa `~/.cache/cattube/images` em uma thread dedicada de manutenção (`64 MiB` / 1.500 arquivos em `low_power`; `256 MiB` / 4.000 arquivos padrão) após a inicialização, além de arquivos `.part` abandonados.
- **Pool de workers separado da manutenção:** separa requisições interativas (`4` workers em `low_power`, `8` padrão) das tarefas de manutenção (`cattube-maint`) para que atualizações e limpeza de cache não travem a abertura, e lê a versão do `yt-dlp` diretamente de `yt-dlp.d/yt_dlp/version.py` sem iniciar o Python.
- **Download de miniaturas em processo (`ureq`):** baixa miniaturas e avatares ausentes em processo com conexões persistentes (decodificando direto da memória e entregando acertos do cache em disco antes de esperar a rede) e usa `mqdefault` e avatares compactos (`240 px`) em `low_power`.
- **Persistência leve na flash e sem recarregar a Home à toa:** compacta arquivos `positions/*.pos` encerrados em um `positions.conf` de 4 campos (retrocompatível com o formato antigo de 2 campos), dispensa `fsync` no cache descartável `home.txt`, não recarrega a Home anônima ao fechar um vídeo, usa debounce real de 200 ms nas sugestões de busca e carrega Histórico e posições fora da thread de UI.

### Estabilidade, compatibilidade e persistência atômica
- **Ciclo de vida e IPC do `mpv` endurecidos:** `PlayerHandle` compartilhado com detecção precoce de saída e `reap_or_kill` com limite de tempo, correção de leituras parciais e EOF no `IpcConn`, fallbacks para versões mais antigas do `mpv` (`loadfile` `< 0.38` e `overlay-add` `< 0.39`), escape ASS e truncamento UTF-8 seguro no OSD e correção da largura da miniatura de *Up Next* (`256 px`).
- **Correções de bugs:** elimina panics ao encolher sugestões de busca ou redimensionar thumbnails com proporções extremas, evita travar o teclado em buscas vazias, corrige o pulo de outros do SponsorBlock no fim do vídeo, corrige a primeira execução com `--play` e salvamento de velocidade ao sair, evita sobrescrita de contas reais ao atualizar perfil, corrige arredondamento em `views_text` e completa traduções do menu de vídeo, selo `LIVE` e idioma das sugestões.
- **Escritas atômicas:** grava checkpoints, `positions.conf` mesclado, histórico de buscas, backdrops borrados e downloads de ferramentas usando arquivos temporários únicos e `rename` atômico. As configurações também são forçadas para o disco e resistem a uma queda de energia.
- **Um player por vez:** os processos do CatTube (o app, `--play`) se coordenam pelo socket do `mpv`. Quem inicia um vídeo salva a posição atual e fecha antes o player do outro.
- **Ferramentas verificadas:** `yt-dlp` e Deno são conferidos com o SHA-256 publicado antes do uso. O Deno é instalado de forma atômica, testado antes de ser ativado, reparado após uma instalação interrompida e verificado para atualização a cada 30 dias.
- **Tarefas em segundo plano resilientes:** uma tarefa que falha não derruba mais o worker, e a tela que a aguardava mostra um erro em vez de ficar carregando para sempre.

### Menos funções
- **Sem login:** o CatTubeLite nunca entra numa conta do YouTube e não guarda tokens. Saíram a tela de login, as contas, as abas Inscrições, Playlists e Canais e as ações Curtir / Não curtir / Inscrever-se / Assistir mais tarde. Um `~/.config/cattube/accounts.conf` que tenha sobrado do CatTube original é ignorado; pode apagá-lo se não precisar mais dele.
- **Sem comentários, chat ao vivo nem contagem de dislikes:** o player não tem mais os botões Comentários e Chat, e o menu do vídeo não consulta mais o Return YouTube Dislike.
- **Sem controle pelo celular:** o app do YouTube no celular não pareia nem transmite mais para o CatTubeLite. Saíram a opção Vincular celular, a variável `CATTUBE_CAST_ENABLED_FILE` e o serviço `--remote`; `--remote`, `--tv-code` e `--pair` agora saem com código 2 sem abrir janela. Um `~/.config/cattube/remote.conf` que tenha sobrado é ignorado.
- **Sem árabe:** a interface fica em inglês ou em português do Brasil, e um `language = ar` que tenha sobrado no `settings.conf` abre em inglês. O suporte a árabe saiu por completo: a fonte embutida Noto Sans Arabic, o reshaping e o layout da direita para a esquerda, e o layout árabe do teclado. Títulos em árabe podem aparecer como quadrados ou com as letras na ordem errada. Na escolha do idioma das legendas, o português ocupa o lugar do árabe.
- **O que continua:** Início (transmissões ao vivo e tópicos populares do YouTube), Histórico (guardado na TV), Busca, páginas de canais e playlists, SponsorBlock e Próximo vídeo.

## Recursos

- **YouTube sem conta.** O Início mostra transmissões ao vivo e tópicos populares, o Histórico lista o que foi assistido na TV e a Busca encontra o resto. Páginas de canais e playlists abrem a partir de qualquer vídeo.
- **Busca** com sugestões com debounce enquanto digita e buscas recentes quando o campo está vazio, exibindo canais correspondentes e grade paginada de vídeos.
- **Player feito para controle remoto.** OK abre os controles, Esquerda/Direita pulam 10 s e Cima vai para a barra de progresso. Qualidade, legendas e faixas de áudio ficam acessíveis no OSD sem perder o ponto do vídeo. Neste fork os vídeos tocam sempre na velocidade normal, e o player fecha com Voltar (não há botão Parar). Só esses controles agem no player: as teclas próprias do `mpv` (volume, números, roda do mouse etc.) ficam desligadas.
- **SponsorBlock.** Pula e marca na barra segmentos de patrocínio, intros, encerramentos e demais categorias escolhidas.
- **Menu do vídeo.** Segure OK em um vídeo para ver visualizações e data, recomeçar ou abrir o canal.
- **Próximo vídeo (Up next).** Inicia o próximo vídeo após contagem regressiva, repete ou para, conforme configurado.
- **Histórico e retomada.** Registra vídeos assistidos e retoma do ponto onde parou.
- **Suporte a mouse.** Aponte para focar e clique para abrir.

## Idiomas

Em **Settings → Playback → Interface language** é possível escolher **Português (Brasil)**. Os controles sobre o vídeo continuam em inglês. Este fork não tem suporte a árabe (veja [Menos funções](#menos-funções)). O idioma das legendas é configurado separadamente.

## Compilação e instalação

O CatTubeLite é compilado a partir do código-fonte. São necessários Rust (edition 2024), `mpv`, `curl` e Python 3.

```sh
git clone https://github.com/victorgmbraga/CatTubeLite.git
cd CatTubeLite
cargo build --release --locked
./target/release/cattube
```

Para rodar a suíte de testes:

```sh
cargo test --locked --workspace
```

Para gerar um pacote para a arquitetura atual e instalá-lo em `~/.local`:

```sh
packaging/tarball.sh
tar -xzf dist/cattube-*-"$(uname -m)".tar.gz -C dist
dist/cattube-*-"$(uname -m)"/install.sh
```

Por padrão, o CatTubeLite é compilado sem o renderizador `wgpu` (Vulkan), em todas as arquiteturas: ele desenha com OpenGL e o binário fica cerca de 31% menor. Para uma compilação de PC com Vulkan, use `cargo build --release --features wgpu` ou `CATTUBE_WGPU=1 packaging/tarball.sh`.

### Nome e logo próprios

Para uma compilação só sua, duas variáveis de compilação trocam o nome e o logo do app. Sem elas, nada muda, e os pacotes publicados pelo CI mantêm sempre o nome e a marca do CatTube.

| Variável | O que muda |
|---|---|
| `CATTUBE_NAME` | O nome no título da janela, no cabeçalho, nas Configurações e no atalho (menus e lançadores) |
| `CATTUBE_LOGO` | Um PNG que aparece no lugar da marca do gato (no cabeçalho e nas telas de canal e de Configurações) e também é instalado como ícone do atalho |

Gere o pacote com elas e instale como de costume (no aparelho de TV também, em `aarch64`):

```sh
CATTUBE_NAME="Meu Tube" CATTUBE_LOGO=/caminho/do/logo.png packaging/tarball.sh
tar -xzf dist/cattube-*-"$(uname -m)".tar.gz -C dist
dist/cattube-*-"$(uname -m)"/install.sh
```

- **O logo:** precisa ser PNG (SVG não é aceito), quadrado ou mais largo que alto, com fundo transparente e pelo menos 256 px de altura. No app, ele aparece na altura da marca original, mantendo as proporções. Se o arquivo não existir ou não for PNG, a compilação para com um erro.
- **Instalar por cima de uma instalação anterior:** o `install.sh` copia só o ícone que o pacote tem (`share/pixmaps/io.github.coderastic.CatTube.png` para um logo, o SVG do gato no caso contrário) e remove o outro, deixado por uma instalação anterior.
- **Um `cargo build` comum:** lê as mesmas variáveis, mas aí só o app muda. O atalho e o ícone vêm do `packaging/tarball.sh`.
- **O que continua igual:** o comando `cattube`, as pastas `~/.config/cattube` e `~/.local/share/cattube` e o identificador interno `io.github.coderastic.CatTube` (app id da janela e nome do arquivo do atalho). As configurações, o histórico e a CatosTV continuam funcionando como antes.

O arquivo do logo fica por sua conta: este repositório não traz nenhum logo de terceiros. Não redistribua uma compilação com o nome ou o logo de outra empresa.

O pacote não inclui `mpv`, `curl` nem Python 3, que precisam estar instalados no aparelho.

Na primeira execução, o CatTubeLite baixa sua própria cópia do [yt-dlp](https://github.com/yt-dlp/yt-dlp) e do [Deno](https://deno.com) (usando o binário nativo ARM64 em `aarch64`) em `~/.local/share/cattube`, verifica os arquivos e os mantém atualizados automaticamente. O yt-dlp também é descompactado ao lado do arquivo único (`yt-dlp.d`, cerca de 16 MB), para que o Python guarde os módulos compilados em vez de compilá-los de novo a cada vídeo; cada vídeo abre mais rápido, cerca de 0,6 s num PC e mais num aparelho de TV.

> Este fork não distribui pacotes Flatpak nem AppImage. Para esses formatos, veja o projeto original, [coderastic/CatTube](https://github.com/coderastic/CatTube).

## Dispositivos de baixo desempenho e diagnóstico

| Variável de ambiente | Valores | Efeito |
|---|---|---|
| `CATTUBE_HWDEC` | um valor de `--hwdec` do mpv (`v4l2m2m-copy`, `vaapi`, `no`…) | Força o decodificador de vídeo. Sem ela, um `hwdec=` no seu `mpv.conf` (`~/.config/mpv/` ou `/etc/mpv/`) é usado como está; sem nenhum dos dois, aparelhos de baixo desempenho recebem `auto-safe` e os demais o padrão do mpv. O `auto-safe` não inclui o `v4l2m2m-copy`, que alguns TV boxes precisam: defina-o no `mpv.conf` ou aqui |
| `CATTUBE_JS_RUNTIME` | `quickjs:/usr/bin/qjs`, `deno:/caminho`, `node:/caminho`… | Substitui o runtime JavaScript passado ao `yt-dlp`. Sem ela, dispositivos com pouca RAM (`<= 1,5 GiB`) priorizam `qjs`/`quickjs` do sistema (mantido residente via `ytdl-jsc-qjs.js`), enquanto aparelhos com mais memória procuram `deno` do sistema, seu próprio `deno`, `qjs`/`quickjs` do sistema e, por fim, `node` |
| `CATTUBE_YTDL_RESIDENT` | `0` | Desliga o `yt-dlp` residente. Por padrão o CatTubeLite mantém um `yt-dlp` (e um Deno ou QuickJS com as funções do player do YouTube prontas) carregado entre vídeos, e o vídeo abre em cerca de um terço do tempo numa TV box, ao custo de ~100 MB de RAM (menor com QuickJS) |
| `CATTUBE_YTDL_DIRECT` | `0` | Desativa a extração direta de streams via servidor residente e volta para o `ytdl_hook` do `mpv` (desativando a abertura paralela em `idle`, troca instantânea de qualidade e pré-resolução do próximo vídeo) |
| `CATTUBE_FASTPATH` | `0` | Desativa o fast-path direto no `/player` do InnerTube `ANDROID_VR` para VODs (que evita Python/`yt-dlp`/JS quando URLs adaptativas diretas funcionam e passam no probe `Range: bytes=0-0`) |
| `CATTUBE_LOW_POWER` | `1` / `0` | Força o perfil de baixo desempenho ou padrão (detectado por RAM, GPU e arquitetura quando não definido). Entre outras coisas, no baixo desempenho os cards usam as imagens de 320×180 do YouTube (`mqdefault`) em vez das de 480×360 com faixas pretas: um terço dos pixels para decodificar, download menor e card mais nítido |
| `CATTUBE_RENDERER` | `glow` / `wgpu` | Força o renderer da interface. Só compilações com `--features wgpu` têm o `wgpu`; nelas o padrão é `glow` em `low_power` e `wgpu` nos demais, com fallback automático se não definido |

Para ver decodificação por hardware, quadros descartados e memória sobre o vídeo, ative **Settings → About → Player statistics**. Também é possível consultar as propriedades `hwdec-current` e `frame-drop-count` no socket IPC do `mpv` em `$XDG_RUNTIME_DIR/cattube-mpv.sock` (`/tmp/cattube-$(id -u)/cattube-mpv.sock` sem essa variável).

## Controles

| Controle remoto / teclado | Nas listas | No player |
|---|---|---|
| Setas | navegar | Esquerda / Direita pulam 10 s (com os controles escondidos, a barra de progresso aparece sozinha por um instante); Cima / Baixo alternam barra, botões e vídeos relacionados |
| Retroceder / Avançar, Anterior / Próximo | | pulam 30 s / 90 s, com a mesma barra de progresso |
| OK, Enter | abrir, reproduzir | mostrar controles, acionar botão focado |
| Segurar OK | menu do vídeo | |
| Voltar, Esc, `GO_BACK` | voltar | ocultar controles e depois parar |

## Linha de comando

| Opção | O que faz |
|---|---|
| `--section NOME` | abre em `home`, `history` ou `search` |
| `--play VIDEO_ID` | reproduz um vídeo e fecha ao terminar |
| `--feed home` | imprime os vídeos do Início (para a tela inicial da TV); outras listas saem com código 2 |
| `--stop` | fecha o player |
| `--windowed` | inicia em janela em vez de tela cheia |

## Arquivos locais

- `~/.config/cattube/` — preferências.
- `~/.local/share/cattube/` — `yt-dlp` (e sua cópia descompactada `yt-dlp.d`, com o lançador `yt-dlp-unpacked`), `deno`, histórico de assistidos, checkpoints, `positions.conf` e buscas recentes. Transmissões ao vivo não guardam posição: sempre começam onde a transmissão está agora.
- `~/.cache/cattube/` — cache de miniaturas (limpo automaticamente).

A pasta de cache também guarda `home.txt`, a última página inicial: ela aparece na hora na próxima abertura (se tiver menos de uma semana e for do mesmo idioma e país) enquanto a nova carrega, e continua na tela se a nova falhar.

## Créditos e agradecimentos

- Fork baseado no [CatTube](https://github.com/coderastic/CatTube), criado por [coderastic](https://github.com/coderastic).
- [yt-dlp](https://github.com/yt-dlp/yt-dlp), [Deno](https://deno.com) e [mpv](https://mpv.io) fazem a extração e reprodução dos vídeos.
- [SponsorBlock](https://sponsor.ajay.app) fornece os trechos a pular.

## Licença

MIT — veja [`LICENSE`](LICENSE). As fontes embutidas Inter (reduzida a latim, grego e cirílico por `packaging/subset-fonts.sh`) e Noto Emoji usam a licença SIL Open Font License (avisos em [`assets/fonts/`](assets/fonts/)). O CatTubeLite não é desenvolvido nem afiliado ao YouTube ou ao Google.
