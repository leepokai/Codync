#!/usr/bin/env python3
"""Build complete it.json with offline Argos en→it."""

from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CATALOG_DIR = ROOT / "apps/ios/Kit/Sources/CodyncKit/Resources/Languages"
SOURCE = CATALOG_DIR / "zh-Hans.json"
OUT = CATALOG_DIR / "it.json"

PLACEHOLDER = re.compile(r"\{[A-Za-z0-9]+\}")

OVERRIDES: dict[str, str] = {
    "English": "English",
    "Follow system": "Segui sistema",
    "Settings": "Impostazioni",
    "Files": "File",
    "Cancel": "Annulla",
    "Back": "Indietro",
    "Pair a computer": "Associa un computer",
    "Personal": "Personale",
    "Privacy": "Privacy",
    "Storage": "Archiviazione",
    "Usage": "Utilizzo",
    "Agents": "Agenti",
    "Apps": "App",
    "Installed": "Installato",
    "Not installed": "Non installato",
    "Set up": "Configura",
    "Today": "Oggi",
    "Yesterday": "Ieri",
    "Stopped.": "Interrotto.",
    "Full conversation": "Conversazione completa",
    "Load earlier messages": "Carica messaggi precedenti",
    "Mark as Read": "Segna come letto",
    "Check for updates": "Controlla aggiornamenti",
    "Remote screen": "Schermo remoto",
    "Fast mode": "Modalità rapida",
    "Theme": "Tema",
    "Support": "Supporto",
    "Add connector": "Aggiungi connettore",
    "New skill": "Nuova skill",
    "Nothing installed": "Niente installato",
    "No setup needed.": "Nessuna configurazione necessaria.",
    "Writing a reply…": "Scrittura risposta…",
    "Resuming session…": "Ripresa sessione…",
    "Ask {name}": "Chiedi a {name}",
    "Remove {name}?": "Rimuovere {name}?",
    "Open {name}": "Apri {name}",
    "Quit {name}": "Esci da {name}",
    "Pair iPhone…": "Associa iPhone…",
    "Restart host": "Riavvia host",
    "2 bots working": "2 bot al lavoro",
    "Search agents, connectors and skills": "Cerca agenti, connettori e skill",
    "Needs approval: Bash": "Richiede approvazione: Bash",
    "Standing instructions": "Istruzioni permanenti",
    "No dreams yet.": "Nessun sogno per ora.",
    "Your coding agents, as teammates.": "I tuoi agenti di coding, come compagni di squadra.",
    "Your coding agents,\nas teammates.": "I tuoi agenti di coding,\ncome compagni di squadra.",
    "App language": "Lingua app",
    "App language…": "Lingua app…",
    "Choose the language for this device. Unsupported system languages use English.": "Scegli la lingua per questo dispositivo. Le lingue di sistema non supportate usano l'inglese.",
    "Bot": "Bot",
    "Bots": "Bot",
    "Connected": "Connesso",
    "Delete bot": "Elimina bot",
    "Agent": "Agente",
    "Manage the team": "Gestisci il team",
    "Skills": "Skill",
    "Said": "Ha detto",
    "resets": "si azzera",
    "resets in {time}": "si azzera tra {time}",
    "after reset": "dopo l'azzeramento",
    "day unit": "g",
    "hour unit": "h",
    "minute unit": "min",
    "Clipboard": "Appunti",
    "Deny": "Nega",
    "Deny: {from} → {to}": "Nega: {from} → {to}",
    "Fallbacks": "Agenti di riserva",
    "Minimal": "Minimo",
    "Offline": "Non in linea",
    "Online": "In linea",
    "Paste": "Incolla",
    "Revoke": "Revoca",
    "Revoke {name}?": "Revocare {name}?",
    "Routines": "Routine",
    "Stop task": "Interrompi attività",
    "Unhide": "Mostra",
    "n add rule · esc back": "n aggiungi regola · esc indietro",
    "n add rule · x delete · K J move · esc back": "n aggiungi regola · x elimina · K J sposta · esc indietro",
    "{name}, 1 bot, {unread} unread": "{name}, 1 bot, {unread} non letti",
    "{name}, {count} bots, {unread} unread": "{name}, {count} bot, {unread} non letti",
    "{size} pt (Default)": "{size} pt (Predefinito)",
    "↑↓ move · ↵ run · esc back": "↑↓ sposta · ↵ esegui · esc indietro",
    " (optional)": " (opzionale)",
    "See the Privacy Policy for details: ": "Consulta l'Informativa sulla privacy per i dettagli: ",
    "Connectors & skills": "Connettori e skill",
    "{count} installed": "{count} installati",
    "%d connected": "%d collegati",
    "%d offline": "%d non in linea",
    "%d/%d connected": "%d/%d collegati",
    "Sign in": "Accedi",
    "Sign out": "Esci",
    "Sign in required": "Accesso richiesto",
    "Signed in": "Accesso effettuato",
    "Signed in.": "Accesso effettuato.",
    "Revoke access": "Revoca accesso",
    "Install goose and run `goose configure`.": "Installa goose ed esegui `goose configure`.",
    "Install DeepSeek Harness (npm install -g @deepseek-ai/dsh) and save your DeepSeek API key in `dsh web` under Settings → Models, or as DEEPSEEK_API_KEY in ~/.dsh/.env.": (
        "Installa DeepSeek Harness (npm install -g @deepseek-ai/dsh) e salva la tua chiave API DeepSeek in `dsh web` sotto Settings → Models, oppure come DEEPSEEK_API_KEY in ~/.dsh/.env."
    ),
}

PLACEHOLDER_FIXES: dict[str, str] = {
    " (optional)": " (opzionale)",
    " Sections: {list}": " Sezioni: {list}",
    "Allow computer control for {name}. macOS may call this Device Control and Data Access or Accessibility. {setup}": (
        "Consenti il controllo del computer per {name}. macOS può chiamarlo Controllo dispositivo e accesso ai dati o Accessibilità. {setup}"
    ),
    "Allow: {from} → {to}": "Consenti: {from} → {to}",
    "Approved through the account · seen {when}": "Approvato tramite l'account · visto {when}",
    "Choose a screen and allow control in the system sharing dialog. {setup}": (
        "Scegli uno schermo e consenti il controllo nella finestra di condivisione di sistema. {setup}"
    ),
    "Codync Screen is not running. {setup}": "Codync Screen non è in esecuzione. {setup}",
    "Next {when}": "Prossimo {when}",
    "Older copy of this computer · {status}": "Copia più vecchia di questo computer · {status}",
    "Paired with a QR code · seen {when}": "Associato con un codice QR · visto {when}",
    "Remote screen is off. {setup}": "Lo schermo remoto è disattivato. {setup}",
    "the Codync cloud refused ({status} {code}): {message}": "Il cloud Codync ha rifiutato ({status} {code}): {message}",
    "{count} bot working": "{count} bot al lavoro",
    "{count} bots working": "{count} bot al lavoro",
    "{name} isn't set up on this computer. {hint}": "{name} non è configurato su questo computer. {hint}",
}


def protect(text: str) -> tuple[str, list[str]]:
    tokens: list[str] = []

    def repl(match: re.Match[str]) -> str:
        tokens.append(match.group(0))
        return f"⟦{len(tokens) - 1}⟧"

    return PLACEHOLDER.sub(repl, text), tokens


def restore(text: str, tokens: list[str]) -> str:
    for index, token in enumerate(tokens):
        text = text.replace(f"⟦{index}⟧", token)
    return text


def ensure_argos() -> None:
    import argostranslate.package
    import argostranslate.translate

    try:
        if argostranslate.translate.translate("Settings", "en", "it"):
            return
    except Exception:
        pass
    argostranslate.package.update_package_index()
    packages = argostranslate.package.get_available_packages()
    pair = next(p for p in packages if p.from_code == "en" and p.to_code == "it")
    argostranslate.package.install_from_path(pair.download())


def translate_key(key: str) -> str:
    import argostranslate.translate

    if key in PLACEHOLDER_FIXES:
        return PLACEHOLDER_FIXES[key]
    if key in OVERRIDES:
        return OVERRIDES[key]
    protected, tokens = protect(key)
    try:
        raw = argostranslate.translate.translate(protected, "en", "it")
    except Exception as error:  # noqa: BLE001
        print(f"WARN {key[:50]!r}: {error}", file=sys.stderr)
        raw = protected
    return restore(raw or key, tokens)


def main() -> int:
    ensure_argos()
    keys = json.loads(SOURCE.read_text(encoding="utf-8"))
    catalog: dict[str, str] = {}
    tmp = OUT.with_suffix(".json.tmp")
    for index, key in enumerate(sorted(keys), start=1):
        catalog[key] = translate_key(key)
        if index % 200 == 0:
            print(f"{index}/{len(keys)}", flush=True)

    payload = json.dumps(catalog, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    tmp.write_text(payload, encoding="utf-8")
    tmp.replace(OUT)
    print(f"Wrote {OUT} ({len(catalog)} entries)")
    return 0 if len(catalog) == len(keys) else 1


if __name__ == "__main__":
    raise SystemExit(main())
