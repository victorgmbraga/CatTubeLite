<p align="center">
  <img src="packaging/io.github.coderastic.CatTube.svg" width="128" alt="CatTubeLite">
</p>

<p align="center">
  <a href="README.md">English</a> · <strong>Português (BR)</strong>
</p>

# CatTubeLite

Um fork leve do [CatTube](https://github.com/coderastic/CatTube) — aplicativo nativo de YouTube para Linux feito para TVs e controles remotos — otimizado para TV boxes Linux ARM64 com hardware limitado (como o BTV9 com GPU Mali-450 e 1–2 GiB de RAM), mantendo compatibilidade total com desktops Linux.

Assim como o CatTube original, abre em tela cheia, funciona com setas, OK e Voltar no controle remoto ou teclado (e controles de videogame no CatosTV), não exibe anúncios e reproduz vídeos via `mpv` com SponsorBlock, Return YouTube Dislike, pareamento pelo celular, comentários e chat ao vivo.

## O que mudou neste fork

O CatTubeLite parte do CatTube `1.0.4` e reúne quatro frentes de mudanças voltadas para desempenho, estabilidade e menor uso de CPU/GPU/RAM:

### Interface plana e renderização leve
- **Tema plano em preto puro (`FLAT`):** remove fundos animados, backdrops borrados, camadas translúcidas de vidro, sombras suaves, brilhos especulares, overdraw redundante e animações de fade em toasts, eliminando composições com transparência caras em GPUs como a Mali-450.
- **Renderer OpenGL (`glow`) por padrão em `low_power`:** usa `glow` em vez de `wgpu` em dispositivos de baixo desempenho, com fallback automático entre `glow` e `wgpu` quando `CATTUBE_RENDERER` não é definido manualmente.
- **Layout de texto e listas mais rápido:** adiciona cache limitado de medição de texto (`fit` / `two_lines` com busca binária), fast path ASCII que pula processamento Unicode BiDi/shaping, menos segmentos de curva com movimento reduzido, culling vertical em Configurações e remoção de `stat()` por frame e clones de `Page` a cada tecla.

### Reprodução otimizada para ARM64 e dispositivos fracos
- **Perfil de baixo desempenho automático (`CATTUBE_LOW_POWER`):** ativado automaticamente com até 3 GiB de RAM, em GPUs Mali-400/450 ou VideoCore IV e em máquinas ARM com menos de 6 GiB (pode ser forçado com `CATTUBE_LOW_POWER=1` ou desativado com `0`). Novas configurações passam a usar **720p SDR**, **H.264** e buffer de **64 MiB** por padrão, preservando preferências já salvas.
- **Deno nativo em ARM64 e prioridade para até 30 fps:** baixa o binário `aarch64-unknown-linux-gnu` do Deno em ARM64 e prioriza streams `<= 30 fps` antes de recorrer a taxas mais altas.
- **`mpv` e OSD mais leves:** adiciona `--hwdec=auto-safe`, `--profile=fast`, `--scale=bilinear`, `--cscale=bilinear`, `--dscale=bilinear`, `--dither-depth=no` e `--deband=no` no modo `low_power`; substitui os desfoques gaussianos pesados em CPU do libass (`\blur140` / `\blur70`) por faixa sólida no OSD; reduz a geometria do spinner; remove o IPC `cattube-input` a cada seta pressionada; troca a escrita por frame de `time-pos` por timer periódico (`10 s` em `low_power`, `5 s` padrão); e mapeia a tecla Voltar do controle remoto (`GO_BACK`) no `mpv`.
- **Atlas de vídeos relacionados em memória e em lotes:** monta o painel de vídeos relacionados diretamente em memória (limite de 12 vídeos em `low_power`, 24 padrão), baixa linhas em paralelo e atualiza o overlay em lotes de 4 thumbnails sem criar arquivos temporários individuais.

### Arquitetura de memória, rede e E/S
- **Caches de imagem limitados em RAM e em disco:** limita texturas em memória (`16 MiB` em `low_power`, `64 MiB` padrão, despejo LRU com prioridade ao foco e lado máximo de `320 px` / `640 px`) e limpa automaticamente `~/.cache/cattube/images` na inicialização (`64 MiB` / 1.500 arquivos em `low_power`; `256 MiB` / 4.000 arquivos padrão), além de arquivos `.part` abandonados.
- **Pool limitado de workers:** substitui a criação ilimitada de threads por tarefa por um pool fixo (`4` workers em `low_power`, `8` padrão).
- **Download de thumbnails em lote e refresh OAuth serializado:** baixa thumbnails ausentes em lote com `curl --parallel` (reaproveitando conexões HTTP/2 + TLS), limita o paralelismo dos feeds de Inscrições (`--parallel-max` 8 em `low_power`, 24 padrão) e serializa renovações de token OAuth para evitar múltiplas requisições simultâneas.
- **Debounce real na busca e E/S não bloqueante:** debounce real de 200 ms no timer de UI com cancelamento por geração nas sugestões de busca; carregamento assíncrono de Histórico e posições fora da thread de UI com merge de checkpoints em `O(N)`; e substituição do polling de 10 ms em subprocessos por espera orientada a EOF com limite de buffer e proteção contra descritores herdados por processos filhos.
- **Daemon `--remote` mais econômico:** intervalo de 1 s (em vez de 200 ms) quando o casting está desativado, verificação de atualização do `yt-dlp` em segundo plano quando já instalado e throttle de 1 Hz no polling de estado do `mpv`.

### Estabilidade, compatibilidade e persistência atômica
- **Ciclo de vida e IPC do `mpv` endurecidos:** `PlayerHandle` compartilhado com detecção precoce de saída e `reap_or_kill` com limite de tempo, correção de leituras parciais e EOF no `IpcConn`, fallbacks para versões mais antigas do `mpv` (`loadfile` `< 0.38` e `overlay-add` `< 0.39`), escape ASS e truncamento UTF-8 seguro no OSD, correção da largura da miniatura de *Up Next* (`256 px`) e guardas de geração para chat ao vivo e comentários.
- **Correções de bugs:** elimina panics ao encolher sugestões de busca ou redimensionar thumbnails com proporções extremas, evita travar o teclado em buscas vazias, corrige o pulo de outros do SponsorBlock no fim do vídeo, corrige a primeira execução com `--play` e salvamento de velocidade ao sair, evita sobrescrita de contas reais ao atualizar perfil, corrige arredondamento em `views_text` e completa traduções do menu de vídeo, selo `LIVE` e idioma das sugestões.
- **Escritas atômicas:** grava contas, pareamento remoto, checkpoints, `positions.conf` mesclado, histórico de buscas, backdrops borrados e downloads de ferramentas usando arquivos temporários únicos e `rename` atômico. Configurações, contas e pareamento remoto também são forçados para o disco e resistem a uma queda de energia.
- **Um player por vez:** o app e o daemon `--remote` se coordenam pelo socket do `mpv`. Quem inicia um vídeo salva a posição atual e fecha antes o player do outro.
- **Ferramentas verificadas:** `yt-dlp` e Deno são conferidos com o SHA-256 publicado antes do uso. O Deno é instalado de forma atômica, testado antes de ser ativado, reparado após uma instalação interrompida e verificado para atualização a cada 30 dias.
- **Tarefas em segundo plano resilientes:** uma tarefa que falha não derruba mais o worker, e a tela que a aguardava mostra um erro em vez de ficar carregando para sempre.

## Recursos

- **Seu YouTube.** Início, Inscrições, Histórico, Playlists e Canais. Login no estilo TV via `google.com/device` no celular, com suporte a múltiplas contas. Sem login, a tela inicial exibe transmissões ao vivo e tópicos populares.
- **Busca** com sugestões com debounce enquanto digita e buscas recentes quando o campo está vazio, exibindo canais correspondentes e grade paginada de vídeos.
- **Player feito para controle remoto.** OK abre os controles, Esquerda/Direita pulam 10 s e Cima vai para a barra de progresso. Qualidade, legendas e faixas de áudio ficam acessíveis no OSD sem perder o ponto do vídeo. Neste fork os vídeos tocam sempre na velocidade normal, e o player fecha com Voltar (não há botão Parar).
- **SponsorBlock.** Pula e marca na barra segmentos de patrocínio, intros, encerramentos e demais categorias escolhidas.
- **Dislikes** via Return YouTube Dislike. Segure OK em um vídeo para curtir/descurtir, inscrever-se, salvar em Assistir mais tarde ou abrir o canal.
- **Comentários e chat ao vivo** ao lado do vídeo.
- **Próximo vídeo (Up next).** Inicia o próximo vídeo após contagem regressiva, repete ou para, conforme configurado.
- **Celular como controle remoto.** No app do YouTube no celular, vá em Configurações → Assistir na TV → Vincular com código da TV e digite o código exibido nas Configurações do CatTubeLite.
- **Histórico e retomada.** Registra vídeos assistidos e retoma do ponto onde parou.
- **Suporte a mouse.** Aponte para focar e clique para abrir.

## Idiomas

Em **Settings → Playback → Interface language** é possível escolher **Português (Brasil)** ou **العربية** (árabe). Os controles sobre o vídeo continuam em inglês. Títulos e buscas em árabe também funcionam corretamente com a interface em inglês, e o teclado virtual inclui layout árabe (**ABC → العربية**). O idioma das legendas é configurado separadamente.

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

Em `aarch64`, o pacote é compilado sem o renderizador `wgpu` (`--no-default-features`), o que deixa o binário cerca de 31% menor. Nesse caso, ele sempre desenha com OpenGL.

O pacote não inclui `mpv`, `curl` nem Python 3, que precisam estar instalados no aparelho.

Na primeira execução, o CatTubeLite baixa sua própria cópia do [yt-dlp](https://github.com/yt-dlp/yt-dlp) e do [Deno](https://deno.com) (usando o binário nativo ARM64 em `aarch64`) em `~/.local/share/cattube`, verifica os arquivos e os mantém atualizados automaticamente.

> Este fork não distribui pacotes Flatpak nem AppImage. Para esses formatos, veja o projeto original, [coderastic/CatTube](https://github.com/coderastic/CatTube).

## Dispositivos de baixo desempenho e diagnóstico

| Variável de ambiente | Valores | Efeito |
|---|---|---|
| `CATTUBE_LOW_POWER` | `1` / `0` | Força o perfil de baixo desempenho ou padrão (detectado por RAM, GPU e arquitetura quando não definido) |
| `CATTUBE_RENDERER` | `glow` / `wgpu` | Força o renderer da interface (padrão `glow` em `low_power` e `wgpu` nos demais, com fallback automático se não definido) |
| `CATTUBE_CAST_ENABLED_FILE` | caminho | Arquivo opcional contendo `cast_enabled = true` ou `false` para ativar/desativar casting em tempo real |

Para ver decodificação por hardware, quadros descartados e memória sobre o vídeo, ative **Settings → About → Player statistics**. Também é possível consultar as propriedades `hwdec-current` e `frame-drop-count` no socket IPC do `mpv` em `$XDG_RUNTIME_DIR/cattube-mpv.sock` (`/tmp/cattube-$(id -u)/cattube-mpv.sock` sem essa variável).

## Controles

| Controle remoto / teclado | Nas listas | No player |
|---|---|---|
| Setas | navegar | Esquerda / Direita pulam 10 s; Cima / Baixo alternam barra, botões e vídeos relacionados |
| OK, Enter | abrir, reproduzir | mostrar controles, acionar botão focado |
| Segurar OK, F2 | menu do vídeo | menu de legendas (fallback) |
| Voltar, Esc, `GO_BACK` | voltar | ocultar controles e depois parar |

## Chave compartilhada de casting

No CatosTV, a opção **Cast to this TV** também controla a conexão de pareamento com o celular. Desligá-la encerra o casting e bloqueia novos pareamentos mantendo a navegação e reprodução locais disponíveis. Outros shells podem definir `CATTUBE_CAST_ENABLED_FILE` apontando para um arquivo com `cast_enabled = true` ou `cast_enabled = false`; a mudança entra em vigor com o app rodando.

## Linha de comando

| Opção | O que faz |
|---|---|
| `--section NOME` | abre em `home`, `subscriptions`, `history`, `playlists`, `channels` ou `search` |
| `--play VIDEO_ID` | reproduz um vídeo e fecha ao terminar |
| `--remote` | fica online em segundo plano para o app do celular, sem abrir janela |
| `--tv-code` | imprime o código para "Vincular com código da TV" |
| `--feed subscriptions` | imprime os vídeos mais recentes das suas inscrições |
| `--windowed` | inicia em janela em vez de tela cheia |

## Arquivos locais

- `~/.config/cattube/` — preferências, contas conectadas (`0600`) e pareamento do controle remoto (`0600`).
- `~/.local/share/cattube/` — `yt-dlp`, `deno`, histórico de assistidos, checkpoints, `positions.conf` e buscas recentes.
- `~/.cache/cattube/` — cache de miniaturas (limpo automaticamente) e metadados do cliente YouTube.

Sair da conta nas Configurações também revoga o token junto ao Google.

## Créditos e agradecimentos

- Fork baseado no [CatTube](https://github.com/coderastic/CatTube), criado por [coderastic](https://github.com/coderastic).
- [yt-dlp](https://github.com/yt-dlp/yt-dlp), [Deno](https://deno.com) e [mpv](https://mpv.io) fazem a extração e reprodução dos vídeos.
- [SponsorBlock](https://sponsor.ajay.app) e [Return YouTube Dislike](https://returnyoutubedislike.com) fornecem os dados comunitários.
- [yt-cast-receiver](https://github.com/patrickkfkan/yt-cast-receiver) serviu de referência para o protocolo de controle remoto do YouTube.

## Licença

MIT — veja [`LICENSE`](LICENSE). As fontes embutidas Inter e Noto Sans Arabic usam a licença SIL Open Font License (avisos em [`assets/fonts/`](assets/fonts/)). O CatTubeLite não é desenvolvido nem afiliado ao YouTube ou ao Google.
