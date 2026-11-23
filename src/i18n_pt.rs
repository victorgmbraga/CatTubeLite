//! The interface in Brazilian Portuguese.

pub fn translate(text: &str) -> &str {
    match text {
        "Home" => "Início",
        "Subscriptions" => "Inscrições",
        "History" => "Histórico",
        "Playlists" => "Playlists",
        "Channels" => "Canais",
        "Search" => "Buscar",
        "Settings" => "Configurações",
        "Accounts" => "Contas",
        "Account" => "Conta",
        "Video" => "Vídeo",
        "Videos" => "Vídeos",
        "Playback" => "Reprodução",
        "About" => "Sobre",
        "Phone remote" => "Celular como controle",
        "Add an account" => "Adicionar conta",
        "YouTube account" => "Conta do YouTube",
        "Sign in with a code" => "Entrar com um código",
        "Sign in to see" => "Entre para ver",
        "Sign in" => "Entrar",
        "Sign in to YouTube" => "Entrar no YouTube",
        "In use" => "Em uso",
        "OK uses it · hold OK to sign out" => "OK para usar · segure OK para sair",
        "Highest quality" => "Qualidade máxima",
        "Lower plays smoother on slow connections" => "Menor fica mais fluida em conexões lentas",
        "Preferred codec" => "Codec preferido",
        "VP9 suits most TVs; AV1 needs newer graphics" => {
            "VP9 serve na maioria das TVs; AV1 exige GPU mais nova"
        }
        "When the video and the TV have it" => "Quando o vídeo e a TV tiverem suporte",
        "Speed" => "Velocidade",
        "Also changed in the player, and kept" => "Também muda no player e fica salva",
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
        "Like" => "Gostei",
        "Dislike" => "Não gostei",
        "Subscribe" => "Inscrever-se",
        "Unsubscribe" => "Cancelar inscrição",
        "Save to Watch later" => "Salvar em Assistir mais tarde",
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
        "Press OK for a new code" => "Pressione OK para um novo código",
        "Getting a code…" => "Obtendo um código…",
        "Waiting for you to enter it…" => "Aguardando você digitar…",
        "Back to cancel" => "Voltar para cancelar",
        "On your phone or computer, go to" => "No celular ou computador, acesse",
        "and enter this code" => "e digite este código",
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
        "Choose the tab to sign in with a code" => "Escolha a aba para entrar com um código",
        "Sign in (the button at the top right) for your recommendations and subscriptions" => {
            "Entre (botão no canto superior direito) para ver recomendações e inscrições"
        }
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
