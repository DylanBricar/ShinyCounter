use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    #[serde(rename = "fr")]
    Fr,
    #[serde(rename = "en")]
    En,
}

impl Lang {
    pub fn label(self) -> &'static str {
        match self {
            Lang::Fr => "Français",
            Lang::En => "English",
        }
    }

    pub fn all() -> &'static [Lang] {
        &[Lang::Fr, Lang::En]
    }
}

pub struct Strings {
    pub app_title: &'static str,
    pub preset: &'static str,
    pub new: &'static str,
    pub duplicate: &'static str,
    pub delete_preset: &'static str,
    pub rename: &'static str,
    pub apply: &'static str,
    pub settings: &'static str,
    pub hide_settings: &'static str,
    pub monitor: &'static str,
    pub source: &'static str,
    pub refresh: &'static str,
    pub language: &'static str,
    pub encounters: &'static str,
    pub armed: &'static str,
    pub locked: &'static str,
    pub start_watching: &'static str,
    pub stop_watching: &'static str,
    pub reset: &'static str,
    pub rearm: &'static str,
    pub color_pickers: &'static str,
    pub pick_on_screen: &'static str,
    pub add_slot: &'static str,
    pub remove_slot_tip: &'static str,
    pub sample: &'static str,
    pub interval: &'static str,
    pub tolerance: &'static str,
    pub preset_notes: &'static str,
    pub obs_overlay: &'static str,
    pub live_http: &'static str,
    pub port: &'static str,
    pub add_as_obs_source: &'static str,
    pub journal: &'static str,
    pub snapshot: &'static str,
    pub clear_log: &'static str,
    pub no_entries: &'static str,
    pub add_note: &'static str,
    pub note_hint: &'static str,
    pub history: &'static str,
    pub no_hits: &'static str,
    pub since_previous: &'static str,
    pub ready: &'static str,
    pub paused: &'static str,
    pub live: &'static str,
    pub cancel: &'static str,
    pub apply_picks: &'static str,
    pub clear_all: &'static str,
    pub pick_on_screen_title: &'static str,
    pub placed: &'static str,
    pub slot_active: &'static str,
    pub slots: &'static str,
    pub empty: &'static str,
    pub manual_rearm: &'static str,
    pub watching_msg: &'static str,
    pub pick_cancelled: &'static str,
    pub capture_error: &'static str,
    pub picker_oob: &'static str,
    pub save_failed: &'static str,
    pub match_count: &'static str,
    pub rearmed: &'static str,
    pub paste_hex: &'static str,
    pub source_monitor: &'static str,
    pub source_window: &'static str,
    pub source_picker: &'static str,
    pub confirm: &'static str,
    pub confirm_reset_title: &'static str,
    pub confirm_reset_msg: &'static str,
    pub confirm_delete_preset_title: &'static str,
    pub confirm_delete_preset_msg: &'static str,
    pub confirm_clear_history_title: &'static str,
    pub confirm_clear_history_msg: &'static str,
    pub action_reset: &'static str,
    pub action_delete: &'static str,
    pub action_clear: &'static str,
    pub accent_color: &'static str,
    pub use_os_accent: &'static str,
    pub session: &'static str,
    pub sessions: &'static str,
    pub open_session: &'static str,
    pub duration: &'static str,
    pub hits_count: &'static str,
    pub hit_singular: &'static str,
    pub hit_plural: &'static str,
    pub no_sessions: &'static str,
    pub page: &'static str,
    pub page_prev: &'static str,
    pub page_next: &'static str,
    pub restore_count: &'static str,
    pub delete_entry: &'static str,
    pub confirm_delete_picker_title: &'static str,
    pub confirm_delete_picker_msg: &'static str,
    pub clear_history: &'static str,
    pub time_to: &'static str,
    pub server_label: &'static str,
    pub server_styled: &'static str,
    pub copy: &'static str,
    pub copied: &'static str,
    pub file_output: &'static str,
    pub file_output_enabled: &'static str,
    pub file_output_path: &'static str,
    pub file_output_browse: &'static str,
    pub file_output_clear: &'static str,
    pub info_file_output: &'static str,
    pub info_server_style: &'static str,
    pub file_output_error: &'static str,
    pub update_check: &'static str,
    pub update_checking: &'static str,
    pub update_uptodate: &'static str,
    pub update_available_title: &'static str,
    pub update_available_msg: &'static str,
    pub update_open: &'static str,
    pub update_download: &'static str,
    pub update_downloading: &'static str,
    pub update_downloaded_title: &'static str,
    pub update_downloaded_msg: &'static str,
    pub update_open_file: &'static str,
    pub update_no_asset: &'static str,
    pub update_later: &'static str,
    pub update_auto_download: &'static str,
    pub update_error: &'static str,
    pub update_snooze_note: &'static str,
    pub info_obs_overlay: &'static str,
    pub info_tolerance: &'static str,
    pub info_interval: &'static str,
    pub info_source: &'static str,
    pub info_accent: &'static str,
    pub info_armed: &'static str,
    pub info_session: &'static str,
    pub info_pick: &'static str,
    pub info_auto_update: &'static str,
}

const FR: Strings = Strings {
    app_title: "Shiny Counter",
    preset: "Préréglage",
    new: "Nouveau",
    duplicate: "Dupliquer",
    delete_preset: "Supprimer",
    rename: "Renommer",
    apply: "Appliquer",
    settings: "Paramètres",
    hide_settings: "Masquer",
    monitor: "Écran",
    source: "Source",
    refresh: "Actualiser",
    language: "Langue",
    encounters: "RENCONTRES",
    armed: "ARMÉ",
    locked: "VERROUILLÉ",
    start_watching: "Démarrer la surveillance",
    stop_watching: "Arrêter la surveillance",
    reset: "Réinitialiser",
    rearm: "Réarmer",
    color_pickers: "Pipettes",
    pick_on_screen: "Choisir à l'écran",
    add_slot: "+ pipette",
    remove_slot_tip: "Supprimer cette pipette",
    sample: "Échantillon",
    interval: "Intervalle",
    tolerance: "Tolérance ±RVB",
    preset_notes: "Notes",
    obs_overlay: "Overlay OBS",
    live_http: "serveur HTTP",
    port: "Port",
    add_as_obs_source: "à ajouter comme Browser Source dans OBS",
    journal: "Journal",
    snapshot: "Sauvegarder le compteur",
    clear_log: "Vider",
    no_entries: "Aucune entrée pour le moment.",
    add_note: "Ajouter",
    note_hint: "Ajoute une note (ex. « Wailord classique, reset »)",
    history: "Historique des resets",
    no_hits: "Aucun reset enregistré pour ce préréglage.",
    since_previous: "depuis le précédent",
    ready: "Prêt à chasser.",
    paused: "En pause.",
    live: "actuel",
    cancel: "Annuler",
    apply_picks: "Valider",
    clear_all: "Tout effacer",
    pick_on_screen_title: "Choisir à l'écran",
    placed: "placé(s)",
    slot_active: "actif",
    slots: "Emplacements",
    empty: "— vide",
    manual_rearm: "Réarmé manuellement.",
    watching_msg: "Surveillance toutes les",
    pick_cancelled: "Sélection annulée.",
    capture_error: "Erreur de capture",
    picker_oob: "pipette hors écran",
    save_failed: "échec sauvegarde",
    match_count: "Shiny détecté — compteur =",
    rearmed: "Réarmé (sortie de l'écran shiny).",
    paste_hex: "Hex (#RRGGBB)",
    source_monitor: "Écran",
    source_window: "Fenêtre",
    source_picker: "Source de capture",
    confirm: "Confirmer",
    confirm_reset_title: "Réinitialiser le compteur ?",
    confirm_reset_msg:
        "Le compteur, l'historique des resets et l'état armé seront effacés. Cette action est irréversible.",
    confirm_delete_preset_title: "Supprimer ce préréglage ?",
    confirm_delete_preset_msg:
        "Toutes les pipettes, couleurs, notes et l'historique de ce préréglage seront perdus définitivement.",
    confirm_clear_history_title: "Effacer l'historique ?",
    confirm_clear_history_msg:
        "L'historique des resets de ce préréglage sera vidé. Le compteur reste inchangé.",
    action_reset: "Réinitialiser",
    action_delete: "Supprimer",
    action_clear: "Effacer",
    clear_history: "Vider l'historique",
    accent_color: "Couleur d'accent",
    use_os_accent: "Couleur du système",
    session: "session",
    sessions: "sessions",
    open_session: "en cours",
    duration: "Durée",
    hits_count: "hits",
    hit_singular: "hit",
    hit_plural: "hits",
    no_sessions: "Aucune session enregistrée. Démarre la surveillance pour créer une session.",
    page: "Page",
    page_prev: "< Précédent",
    page_next: "Suivant >",
    restore_count: "Restaurer ce compteur",
    delete_entry: "Supprimer cette entrée",
    confirm_delete_picker_title: "Supprimer cette pipette ?",
    confirm_delete_picker_msg: "La position et la couleur cible de cette pipette seront effacées.",
    time_to: "à",
    update_check: "Vérifier les mises à jour",
    update_checking: "Vérification…",
    update_uptodate: "À jour",
    update_available_title: "Mise à jour disponible",
    update_available_msg:
        "Une nouvelle version de Shiny Counter est disponible. Tu peux ouvrir la page GitHub pour télécharger l'installeur correspondant à ton système.",
    update_open: "Ouvrir GitHub",
    update_download: "Télécharger",
    update_downloading: "Téléchargement…",
    update_downloaded_title: "Téléchargement terminé",
    update_downloaded_msg: "L'installeur a été enregistré dans ton dossier Téléchargements. Ferme Shiny Counter, puis ouvre le fichier pour installer la nouvelle version. Tes préréglages et ton historique sont conservés (config stockée dans %APPDATA%\\ShinyCounter).",
    update_open_file: "Ouvrir le fichier",
    update_no_asset: "Aucun installeur n'est disponible pour ta plateforme. Ouvre la page GitHub pour télécharger manuellement.",
    update_later: "Plus tard (7 jours)",
    update_auto_download: "Télécharger automatiquement les mises à jour",
    update_error: "Échec de la vérification",
    update_snooze_note: "Cette version sera proposée à nouveau dans 7 jours.",
    server_label: "Serveur HTTP",
    server_styled: "Avec style",
    copy: "Copier",
    copied: "Copié",
    file_output: "Sortie fichier",
    file_output_enabled: "Activée",
    file_output_path: "Chemin du fichier",
    file_output_browse: "Parcourir…",
    file_output_clear: "Effacer",
    info_file_output:
        "Écrit le compteur (nombre seul) dans un fichier texte à chaque incrément. Pointe une OBS « Source de texte (depuis un fichier) » dessus pour afficher le compteur sans serveur HTTP. L'écriture est atomique : les lecteurs voient toujours une valeur cohérente.",
    info_server_style:
        "Si coché, l'URL renvoie une page HTML stylisée qui se met à jour toute seule (idéal pour OBS Browser Source). Sinon, l'URL renvoie juste le nombre brut en texte.",
    file_output_error: "Échec d'écriture du fichier",
    info_obs_overlay:
        "Petit serveur HTTP local (127.0.0.1) qui sert le compteur. Ajoute http://127.0.0.1:7878/ comme Browser Source dans OBS pour afficher le numéro sur ton stream — la page se rafraîchit toute seule 4×/seconde.",
    info_tolerance:
        "Tolérance ±RVB pour le matching pixel. ±0 = correspondance exacte, ±20 (défaut) = bon compromis pour les streams compressés, ±50+ pour les vidéos très bruitées.",
    info_interval:
        "Intervalle entre deux échantillonnages (en millisecondes). 100 ms = 10 lectures/seconde, suffisant pour la plupart des jeux. Augmente si tu utilises beaucoup de CPU.",
    info_source:
        "Choisis l'écran à surveiller, ou directement une fenêtre — la capture par fenêtre fonctionne même si la fenêtre est cachée derrière une autre (utile pour les émulateurs en arrière-plan).",
    info_accent:
        "Couleur d'accent du préréglage. Par défaut, on prend celle du système d'exploitation. Tu peux la personnaliser par préréglage pour repérer rapidement la chasse active.",
    info_armed:
        "ARMÉ = le compteur peut s'incrémenter au prochain match. VERROUILLÉ = en attente que toutes les pipettes voient une couleur différente avant de pouvoir compter à nouveau. Évite les doublons sur un écran qui ne change pas.",
    info_session:
        "Une session débute quand tu cliques sur « Démarrer la surveillance » et se termine au stop. Chaque hit (rencontre détectée) est rattaché à la session en cours.",
    info_pick:
        "Capture un screenshot de la source choisie, puis clique 3 fois dessus pour placer les pipettes. Chaque clic enregistre la couleur du pixel ciblé comme cible à matcher.",
    info_auto_update:
        "Si activé, Shiny Counter télécharge directement la nouvelle version dans ton dossier Téléchargements dès qu'elle est disponible, sans demander confirmation. Tes préréglages restent intacts (config stockée dans %APPDATA%\\ShinyCounter).",
};

const EN: Strings = Strings {
    app_title: "Shiny Counter",
    preset: "Preset",
    new: "New",
    duplicate: "Duplicate",
    delete_preset: "Delete",
    rename: "Rename",
    apply: "Apply",
    settings: "Settings",
    hide_settings: "Hide",
    monitor: "Monitor",
    source: "Source",
    refresh: "Refresh",
    language: "Language",
    encounters: "ENCOUNTERS",
    armed: "ARMED",
    locked: "LOCKED",
    start_watching: "Start watching",
    stop_watching: "Stop watching",
    reset: "Reset",
    rearm: "Rearm",
    color_pickers: "Color pickers",
    pick_on_screen: "Pick on screen",
    add_slot: "+ slot",
    remove_slot_tip: "Remove this picker",
    sample: "Sample",
    interval: "Interval",
    tolerance: "Tolerance ±RGB",
    preset_notes: "Notes",
    obs_overlay: "OBS overlay",
    live_http: "live HTTP",
    port: "Port",
    add_as_obs_source: "add as OBS browser source",
    journal: "Journal",
    snapshot: "Snapshot count",
    clear_log: "Clear",
    no_entries: "No entries yet.",
    add_note: "Add",
    note_hint: "Add a note (e.g. \"caught regular Wailord, reset\")",
    history: "Reset history",
    no_hits: "No reset logged for this preset yet.",
    since_previous: "since previous",
    ready: "Ready to hunt.",
    paused: "Paused.",
    live: "live",
    cancel: "Cancel",
    apply_picks: "Apply",
    clear_all: "Clear all",
    pick_on_screen_title: "Pick on screen",
    placed: "placed",
    slot_active: "active",
    slots: "Slots",
    empty: "— empty",
    manual_rearm: "Manually rearmed.",
    watching_msg: "Watching every",
    pick_cancelled: "Pick cancelled.",
    capture_error: "Capture error",
    picker_oob: "picker out of bounds",
    save_failed: "save failed",
    match_count: "Match — count =",
    rearmed: "Rearmed (screen left shiny state).",
    paste_hex: "Hex (#RRGGBB)",
    source_monitor: "Screen",
    source_window: "Window",
    source_picker: "Capture source",
    confirm: "Confirm",
    confirm_reset_title: "Reset counter?",
    confirm_reset_msg:
        "The counter, reset history and armed state will be cleared. This cannot be undone.",
    confirm_delete_preset_title: "Delete this preset?",
    confirm_delete_preset_msg:
        "All pickers, colors, notes and history for this preset will be lost permanently.",
    confirm_clear_history_title: "Clear history?",
    confirm_clear_history_msg:
        "The reset history for this preset will be wiped. The counter is preserved.",
    action_reset: "Reset",
    action_delete: "Delete",
    action_clear: "Clear",
    clear_history: "Clear history",
    accent_color: "Accent color",
    use_os_accent: "OS default",
    session: "session",
    sessions: "sessions",
    open_session: "active",
    duration: "Duration",
    hits_count: "hits",
    hit_singular: "hit",
    hit_plural: "hits",
    no_sessions: "No session recorded yet. Start watching to create one.",
    page: "Page",
    page_prev: "< Prev",
    page_next: "Next >",
    restore_count: "Restore this count",
    delete_entry: "Delete this entry",
    confirm_delete_picker_title: "Delete this picker?",
    confirm_delete_picker_msg: "The position and target color of this picker will be erased.",
    time_to: "to",
    update_check: "Check for updates",
    update_checking: "Checking…",
    update_uptodate: "Up to date",
    update_available_title: "Update available",
    update_available_msg:
        "A new version of Shiny Counter is available. Open the GitHub release page to download the installer for your platform.",
    update_open: "Open GitHub",
    update_download: "Download",
    update_downloading: "Downloading…",
    update_downloaded_title: "Download complete",
    update_downloaded_msg: "The installer has been saved to your Downloads folder. Close Shiny Counter, then open the file to install the new version. Your presets and history are preserved (config lives in %APPDATA%\\ShinyCounter).",
    update_open_file: "Open file",
    update_no_asset: "No installer is available for your platform. Open the GitHub page to download manually.",
    update_later: "Later (7 days)",
    update_auto_download: "Download new releases automatically",
    update_error: "Update check failed",
    update_snooze_note: "This version will be offered again in 7 days.",
    server_label: "HTTP Server",
    server_styled: "Styled",
    copy: "Copy",
    copied: "Copied",
    file_output: "File output",
    file_output_enabled: "Enabled",
    file_output_path: "File path",
    file_output_browse: "Browse…",
    file_output_clear: "Clear",
    info_file_output:
        "Writes the counter (just the number) to a text file every time it changes. Point an OBS \"Text from file\" source at it for a no-server overlay. Writes are atomic so readers always see a consistent value.",
    info_server_style:
        "When checked, the URL serves a styled HTML page that auto-refreshes (perfect for an OBS Browser Source). Otherwise the URL returns just the raw count as plain text.",
    file_output_error: "File write failed",
    info_obs_overlay:
        "Tiny local HTTP server (127.0.0.1) serving the counter. Add http://127.0.0.1:7878/ as an OBS Browser Source — the page polls itself 4 times per second so the number updates live.",
    info_tolerance:
        "RGB tolerance for pixel matching. ±0 = exact match, ±20 (default) is a good fit for compressed streams, ±50+ for noisy footage.",
    info_interval:
        "Sampling cadence in milliseconds. 100 ms = 10 reads/second, enough for most games. Raise it if CPU usage is a concern.",
    info_source:
        "Pick a monitor to watch, or a specific window — window capture keeps working even if the window is hidden behind another (useful for background emulators).",
    info_accent:
        "Preset accent color. Defaults to the OS accent. Override it per preset to spot the current hunt at a glance.",
    info_armed:
        "ARMED = the counter can fire on the next match. LOCKED = waiting for all pickers to read a different color before it can count again. Prevents double-counts on a static encounter screen.",
    info_session:
        "A session opens when you press “Start watching” and closes on stop. Every hit (detected encounter) belongs to the active session.",
    info_pick:
        "Captures a screenshot of the chosen source, then prompts you to click 3 spots. Each click records that pixel's color as a target to match.",
    info_auto_update:
        "When enabled, Shiny Counter downloads the new version straight to your Downloads folder as soon as it's available, with no prompt. Your presets stay intact (config lives in %APPDATA%\\ShinyCounter).",
};

pub fn strings(lang: Lang) -> &'static Strings {
    match lang {
        Lang::Fr => &FR,
        Lang::En => &EN,
    }
}

/// Returns the singular form when `n` should be considered singular for the
/// given language, otherwise the plural form. French follows the rule
/// `n <= 1 => singular`, English follows `n == 1 => singular`.
pub fn pluralize<'a>(lang: Lang, n: usize, singular: &'a str, plural: &'a str) -> &'a str {
    match lang {
        Lang::Fr => {
            if n <= 1 {
                singular
            } else {
                plural
            }
        }
        Lang::En => {
            if n == 1 {
                singular
            } else {
                plural
            }
        }
    }
}

pub fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r, g, b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_with_or_without_hash() {
        assert_eq!(parse_hex("#FFA500"), Some((255, 165, 0)));
        assert_eq!(parse_hex("ffa500"), Some((255, 165, 0)));
        assert_eq!(parse_hex("  #123456 "), Some((0x12, 0x34, 0x56)));
    }

    #[test]
    fn rejects_invalid_hex() {
        assert_eq!(parse_hex("zzz"), None);
        assert_eq!(parse_hex("#1234"), None);
        assert_eq!(parse_hex(""), None);
    }
}
