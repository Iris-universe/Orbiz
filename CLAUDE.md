# Orbiz (version Rust)

Orbe flottant pour Windows : un clic ouvre un anneau de raccourcis ; chat IA (API OpenAI) qui pilote le navigateur IrisBrowser et les apps compatibles iris-link ; notes ; mini navigateur. **Rust + Tauri 2**, pages HTML/JS dans `ui/`. Lancement : `Orbiz.vbs` (lance `target\release\orbiz.exe`). Ne pas lancer l'exe pour tester sans demander à l'utilisateur.

## Données (`%APPDATA%\orbiz`, même dossier que l'ancienne version Electron)
- `actions.json` : **boutons de l'orbe** (créé avec les boutons par défaut de `src/actions.rs` ; les raccourcis perso s'ajoutent ici, jamais dans le code). Types : `overlay`, `iris`, `irisbrowser`, `exe`, `open`.
- `chat.json` (clé API chiffrée DPAPI ; une clé safeStorage de l'ancienne version est reprise et rechiffrée), `usage.json` (dépenses estimées), `chat-errors.log`, `notes.json`, `state.json` (position de l'orbe), `overlays.json` (positions des fenêtres), `browser-profile/` (session du mini navigateur).

## Rust (`src/`)
- `main.rs` : fenêtre de l'orbe, icône de notification + menu (recentrer, fenêtres, démarrage avec Windows = clé `HKCU\…\Run\Orbiz`, quitter avec animation), raccourcis globaux (Ctrl+Maj+Espace chat, Ctrl+Maj+N notes, Ctrl+Maj+B mini navigateur), commandes Tauri, serveur iris-link de l'assistant.
- `orb.rs` : physique de l'orbe (suivi du glisser, lancer, rebonds, ressort) en coordonnées logiques à 60 i/s dans un fil à part. Les clics traversent la fenêtre : Rust lit la souris (GetCursorPos) et l'envoie à la page (`cursor`), qui dit si elle survole une zone `.hit` (`orb_set_ignore`). Écrans mis en cache (jamais d'appel bloquant au fil principal sous le verrou de l'orbe).
- `overlays.rs` : fenêtres chat / notes / browser (sans cadre, toujours devant, fermer = cacher). **Transparence réglable partout** (curseur `.op-slider` de `ui/overlay.css` dans chaque barre du haut, valeur dans `overlays.json` → `opacity`) : chat et notes = opacité du fond (`--a`, le texte reste lisible) ; mini navigateur = toute la fenêtre (WS_EX_LAYERED). Celle-ci n'est pas « transparent » au sens de Tauri (DirectComposition, où layered est sans effet) et tao retire WS_EX_LAYERED quand il recalcule les styles : `reapply_opacity` après affichage, focus et redimensionnement.
- `chat.rs` : boucle agentique OpenAI Responses en streaming (SSE), outils du navigateur (`browser_tools.json`) + outils des apps (`iris_link::host`), compactage du contexte, limite de coût par message, tarifs, journal d'erreurs. Prompt système : `system_prompt.txt`.
- `browser.rs` : pipe d'IrisBrowser (`\.\pipe\IrisBrowserSingleInstance`), pipe du mini navigateur (`\.\pipe\OrbizBrowserOverlay`), webview enfant du mini navigateur.
- Briques (`../iris-briques`) : `iris-link` (côté app et `host` = côté assistant), `iris-store`, `iris-secrets`.

## Pièges (vus en testant)
- Une commande Tauri **synchrone** tourne sur le fil principal : y appeler `add_child` (qui attend le fil principal) bloque tout → commandes `async` (`browser_view`).
- Filtre `on_navigation` des fenêtres : laisser passer `about:blank` (chargé avant la page).
- Menu contextuel : Windows le referme aussitôt si sa fenêtre n'est pas au premier plan → `set_focus` puis `popup_menu` (`orb_context_menu`).
- Évènement `overlay:shown` : peut partir avant que la page écoute au premier affichage (le mini navigateur crée aussi sa webview au chargement).

## Tester
Lancer avec `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9333` puis piloter les pages par CDP (`http://127.0.0.1:9333/json`) : évaluer du JS, lire la console. La webview du mini navigateur (autre profil) n'y apparaît pas. Remettre ensuite les données de l'utilisateur comme avant (notes, transparences).

## Pages (`ui/`)
- `bridge.js` : remplace les preloads (`window.orbiz`, `overlay`, `chat`, `notes`, `browserOverlay`) + déplacement des fenêtres par les zones `-webkit-app-region: drag`.
- `orb/` (orbe animé, yeux, humeurs), `chat/`, `notes/`, `browser/` : reprises de la version Electron.

## Briques partagées : où elles sont prises
`Cargo.toml` les prend sur GitHub (`github.com/Iris-universe/iris-briques`) ; `.cargo/config.toml` (local, non versionné) les redirige vers `../iris-briques` pour développer. Modifier une brique = la modifier dans `iris-briques`, puis la pousser.
