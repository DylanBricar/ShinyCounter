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
