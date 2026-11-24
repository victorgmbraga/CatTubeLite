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
        "Phone remote" => "Celular como controle",
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
        "Small" => "Pequeno",
        "Medium" => "Médio",
        "Large" => "Grande",
        "Country" => "País",
        "For recommendations, trending and news" => "Para recomendações, em alta e notícias",
        "Skip sponsors and more" => "Pular patrocínios e mais",
        "Segments marked by SponsorBlock viewers" => {
            "Trechos marcados por usuários do SponsorBlock"
        }
        "Link a phone" => "Vincular celular",
        "Pick and control videos from the YouTube app" => {
            "Escolha e controle vídeos pelo app do YouTube"
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
        "Getting a code…" => "Obtendo um código…",
        "Link your phone" => "Vincule seu celular",
        "Link with TV code" => "Vincular com código da TV",
        "In the YouTube app on your phone, open Settings → Watch on TV" => {
            "No app do YouTube no celular, abra Configurações → Assistir na TV"
        }
        "→ Link with TV code, and enter" => "→ Vincular com código da TV e digite",
        "The code changes every few minutes. A linked phone stays linked." => {
            "O código muda a cada poucos minutos. O celular vinculado continua vinculado."
        }
        "Then pick a video on the phone: it plays here, and the phone stays in step." => {
            "Depois escolha um vídeo no celular: ele toca aqui e os dois ficam em sincronia."
        }
        "OK for a new code · Back to close" => "OK para novo código · Voltar para fechar",
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
        _ => text,
    }
}
