//! The interface in Brazilian Portuguese.

pub fn translate(text: &str) -> &str {
    match text {
        "Home" => "Início",
        "History" => "Histórico",
        "Channels" => "Canais",
        "Search" => "Buscar",
        "Settings" => "Configurações",
        "Video" => "Vídeo",
        "Videos" => "Vídeos",
        "Playback" => "Reprodução",
        "About" => "Sobre",
        "Highest quality" => "Qualidade máxima",
        "Lower plays smoother on slow connections" => "Menor fica mais fluida em conexões lentas",
        "Preferred codec" => "Codec preferido",
        "VP9 suits most TVs; AV1 needs newer graphics" => {
            "VP9 serve na maioria das TVs; AV1 exige GPU mais nova"
        }
        "When the video and the TV have it" => "Quando o vídeo e a TV tiverem suporte",
        "Subtitles" => "Legendas",
        "Subtitle language" => "Idioma das legendas",
        "Interface language" => "Idioma da interface",
        "When a video ends" => "Quando um vídeo termina",
        "Play the next video" => "Reproduzir o próximo",
        "Play it again" => "Repetir",
        "Stop" => "Parar",
        "Buffer" => "Buffer",
        "Bigger rides out a shaky connection, smaller uses less memory" => {
            "Maior aguenta conexão instável, menor usa menos memória"
        }
        "Minimum" => "Mínimo",
        "Small" => "Pequeno",
        "Medium" => "Médio",
        "Large" => "Grande",
        "Best" => "Melhor",
        "Watched" => "Assistido",
        "Resume at" => "Continuar de",
        "No internet connection" => "Sem conexão com a internet",
        "YouTube took too long to answer" => "O YouTube demorou demais para responder",
        "YouTube said no" => "O YouTube recusou o pedido",
        "YouTube didn't answer properly" => "O YouTube não respondeu corretamente",
        "That can't be opened" => "Não foi possível abrir",
        "Something went wrong, try again" => "Algo deu errado, tente novamente",
        "Could not save the settings" => "Não foi possível salvar as configurações",
        "Could not go back to the video" => "Não foi possível voltar ao vídeo",
        "Getting the player ready, try again in a moment" => {
            "Preparando o player, tente novamente em instantes"
        }
        "Could not download yt-dlp" => "Não foi possível baixar o yt-dlp",
        "mpv is not installed" => "O mpv não está instalado",
        "Country" => "País",
        "For recommendations, trending and news" => "Para recomendações, em alta e notícias",
        "Skip sponsors and more" => "Pular patrocínios e mais",
        "Segments marked by SponsorBlock viewers" => {
            "Trechos marcados por usuários do SponsorBlock"
        }
        "Player statistics" => "Estatísticas do player",
        "Decoder, dropped frames and memory over the video" => {
            "Decodificador, quadros perdidos e memória sobre o vídeo"
        }
        "Update the player" => "Atualizar o player",
        "Updating…" => "Atualizando…",
        "Now playing" => "Reproduzindo agora",
        "Return to video" => "Voltar ao vídeo",
        "Playing" => "Reproduzindo",
        "Play" => "Reproduzir",
        "Start over" => "Começar do início",
        "Go to channel" => "Ir para o canal",
        "No channel for this one" => "Este vídeo não tem canal",
        "For you" => "Para você",
        "People also watched" => "Outras pessoas também assistiram",
        "Shorts" => "Shorts",
        "Recent searches" => "Buscas recentes",
        "Search YouTube" => "Buscar no YouTube",
        "Search to get started" => "Busque algo para começar",
        "Nothing found" => "Nada encontrado",
        "Nothing here yet" => "Nada por aqui ainda",
        "Down for results" => "Para baixo para ver os resultados",
        "Press OK to try again" => "Pressione OK para tentar de novo",
        "Getting the player ready (first start)…" => "Preparando o player (primeiro uso)…",
        "space" => "espaço",
        "delete" => "apagar",
        "LIVE" => "AO VIVO",
        "Short" => "Short",
        "Sponsor" => "Patrocínio",
        "Unpaid or self promotion" => "Autopromoção ou promoção não paga",
        "Reminder to subscribe" => "Lembrete para se inscrever",
        "Intro" => "Introdução",
        "Credits and end cards" => "Créditos e telas finais",
        "Preview or recap" => "Prévia ou recapitulação",
        "Non-music part in a music video" => "Trecho sem música em clipe musical",
        "Off-topic filler" => "Conteúdo fora do assunto",
        "Hook or greeting" => "Chamada ou saudação",
        "Could not play this video" => "Não foi possível reproduzir este vídeo",
        "The video took too long to start" => "O vídeo demorou demais para começar",
        "This video is not available" => "Este vídeo não está disponível",
        "YouTube blocked playback; try updating the player in Settings" => {
            "O YouTube bloqueou a reprodução; tente atualizar o player em Configurações"
        }
        "Could not reach YouTube" => "Não foi possível conectar ao YouTube",
        "This quality or codec is not available for this video" => {
            "Esta qualidade ou codec não está disponível para este vídeo"
        }
        "Could not start video playback; check quality or codec in Settings" => {
            "Não foi possível iniciar o vídeo; verifique a qualidade ou o codec em Configurações"
        }
        _ => text,
    }
}
