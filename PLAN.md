# FPV Editor — Projektplan

Ein nativer Video-Editor für FPV-Drohnenpiloten in Rust: Stabilisierung (gyrobasiert wie
Gyroflow), klassischer Schnitt/Timeline, moderne UI, und durchgängige AI-Fähigkeit — sowohl
von außen fernsteuerbar (Agents) als auch mit einem eingebauten AI-Assistenten, der gegen
einen konfigurierbaren OpenAI-kompatiblen Endpunkt läuft.

## 1. Ziele & Nicht-Ziele

**Ziele**
- Video-Stabilisierung speziell für FPV-Footage (Gyro-/Blackbox-Daten, Lens-Korrektur, Horizon Lock)
- Vollwertiger Schnitt: Multi-Track-Timeline, Trim/Cut/Ripple, Übergänge, Speed-Ramping, Farbkorrektur/LUTs
- FPV-spezifische Extras: OSD/Telemetry-Overlay, Blackbox-Log-Import, Kamera-/Linsenprofile
- AI von zwei Seiten andockbar:
  1. **Extern**: Agents (z. B. Claude Code, eigene Scripts) steuern den Editor headless/parallel zur GUI
  2. **Intern**: Ein Chat-/Agent-Panel in der App selbst, das dieselben Editier-Funktionen als Tools aufruft
- Konfigurierbarer OpenAI-kompatibler Endpunkt (Base-URL + API-Key + Modellname) → funktioniert mit OpenAI, Azure OpenAI, Ollama, LM Studio, vLLM etc.
- Modernes, natives UI (keine 1998er-Ästhetik)

**Nicht-Ziele (v1)**
- Kein Cloud-Rendering/Kollaborations-Backend
- Keine mobile App
- Kein eigener Codec — auf ffmpeg/GPU-Pipelines setzen statt Encoder neu erfinden

## 2. Architektur-Überblick

Kernidee: **eine einzige Command-API** ist die "Quelle der Wahrheit" für alle Timeline-Mutationen
(Clip hinzufügen, schneiden, stabilisieren, LUT anwenden, exportieren …). GUI, interner AI-Agent
und externe Agents rufen alle dieselben Commands auf — das gibt automatisch Konsistenz,
Undo/Redo und Skriptbarkeit "for free".

```
                     ┌─────────────────────┐
                     │   Command / Event    │   (fpv-core)
                     │   Bus + Undo/Redo     │
                     └──────────▲───────────┘
           ┌─────────────┬──────┼──────┬─────────────────┐
           │             │      │      │                 │
     ┌─────┴────┐  ┌─────┴───┐ │ ┌────┴─────┐     ┌──────┴──────┐
     │ Tauri UI │  │ interner│ │ │ MCP-Server│     │  CLI (fpv)  │
     │ (Frontend)│  │ AI-Agent│ │ │ (extern)  │     │ (Scripting) │
     └──────────┘  └─────────┘ │ └───────────┘     └─────────────┘
                                │
                    externe Agents (Claude Code, etc.)
```

### Crate-Layout (Cargo Workspace)

| Crate | Zweck |
|---|---|
| `fpv-core` | Projekt-/Timeline-Datenmodell, Command-Bus, Undo/Redo, Serialisierung (Projektdatei) |
| `fpv-media` | Decode/Encode via ffmpeg, Frame-Pipeline, Proxy-Generierung |
| `fpv-stabilize` | Gyro-/Blackbox-basierte Stabilisierung, Lens-Profile, Horizon Lock, optischer Fallback |
| `fpv-gpu` | wgpu-basiertes Rendering: Compositing, Color-Grading/LUTs, Warp für Stabilisierung |
| `fpv-ai` | OpenAI-kompatibler Client, Agent-Loop mit Tool-Calling gegen die Command-API |
| `fpv-mcp` | MCP-Server: exponiert Editor-Funktionen als Tools für externe Agents |
| `fpv-cli` | Headless-CLI für Automation/Skripte (nutzt dieselbe Core-API) |
| `fpv-app` | Tauri-Shell, IPC-Commands, bindet alle Crates zusammen |
| `frontend/` | UI (Svelte/React + Tailwind), Timeline-Canvas, Preview-Player |

## 3. Tech-Stack

- **Sprache/Runtime**: Rust, `tokio` für Async
- **UI-Shell**: Tauri 2 (Rust-Backend, Webview-Frontend) — schnellster Weg zu einem wirklich
  modernen Look (Tailwind, shadcn-artige Komponenten, saubere Animationen), Video-Preview über
  GPU-Textur/Canvas-Streaming
  - *Alternative, falls reines Rust-UI gewünscht ist*: `iced` (wgpu-basiert, kein Webview) — weniger
    Design-Flexibilität, dafür ein einziges Rust-Toolchain ohne Webview-Abhängigkeit
- **Video I/O**: `ffmpeg-next` (Bindings) für Decode/Encode/Muxing
- **GPU-Pipeline**: `wgpu` für Color-Grading, LUT-Anwendung, Stabilisierungs-Warp, Compositing
- **Stabilisierung**: Eigene Implementierung nach Gyroflow-Prinzip (Gyro-Integration, Rolling-Shutter-
  Korrektur, Lens-Profile-Datenbank) — ggf. Konzepte/Format von `gyroflow-core` übernehmen
  (Lizenz prüfen: Gyroflow ist GPL-3.0 → bei Code-Übernahme wird das eigene Projekt GPL-3.0-pflichtig;
  alternativ eigenständig nach Paper/Algorithmus neu implementieren, um Lizenzfreiheit zu behalten)
- **AI-Client**: `async-openai` (spricht OpenAI-API-Dialekt, funktioniert gegen jeden
  OpenAI-kompatiblen Endpunkt via konfigurierbarer `base_url`)
- **Externe Agent-Schnittstelle**: MCP-Server (Rust-SDK, z. B. `rmcp`) — dadurch direkt kompatibel
  mit Claude Code & Co., zusätzlich optional ein schlichtes REST-Interface für Nicht-MCP-Clients
- **Projektdatei**: JSON oder RON, versionierbar, menschenlesbar (für Debug/Diff)

## 4. AI-Integration im Detail

### 4.1 Einstellungen
- UI-Panel "AI-Provider": Base-URL, API-Key, Modellname, optionale Header
- Presets für gängige lokale Server (Ollama, LM Studio) plus "Custom"
- Verbindungstest-Button

### 4.2 Tool-Definitionen (gemeinsam für internen Agent & MCP)
Zentrale Tool-Liste, einmal definiert, zweimal exponiert (intern als Function-Calling-Schema,
extern als MCP-Tools), z. B.:
- `list_clips`, `get_timeline_state`
- `trim_clip`, `split_clip`, `add_clip`, `reorder_clips`
- `apply_stabilization(clip_id, profile)`
- `apply_lut(clip_id, lut_path)`, `set_speed_ramp(clip_id, keyframes)`
- `add_text_overlay`, `add_osd_overlay(telemetry_source)`
- `render_export(preset)`

### 4.3 Interner Agent
- Chat-Panel in der App, das gegen die Timeline "sieht" (Kontext = aktueller Projekt-State)
- Nutzt die gleichen Tools wie der MCP-Server → ein Prompt wie "schneide alle Clips unter 2 Sekunden raus und stabilisiere den Rest" ist direkt umsetzbar

### 4.4 Externe Steuerung
- MCP-Server läuft optional im Hintergrund der App (oder headless via `fpv-cli mcp-serve`)
- Externe Agents können so ganze Schnittabläufe automatisieren, ohne GUI-Interaktion

## 5. FPV-spezifische Features

- **Blackbox-/Gyro-Import**: Betaflight/INAV-Logs, eingebettete Gyro-Metadaten (GoPro-ähnlich, falls vorhanden), Kamera-Latenz-Sync
- **Lens-/Kamera-Profile**: Datenbank für gängige FPV-Cams (Caddx, RunCam, DJI O3/O4 etc.), FOV-/Distortion-Korrektur
- **Horizon Lock & dynamisches FOV** (wie Gyroflow "stabilization strength" + "smoothness")
- **OSD-Overlay**: Rendering von Betaflight-OSD/WalkSnail/HDZero-Telemetriedaten über das Footage
- **Musik-Sync/Speed-Ramping**: Beat-Erkennung (optional AI-gestützt) für Freestyle-Edits

## 6. Roadmap (Phasen)

1. **Fundament**: Workspace-Setup, Tech-Spikes (ffmpeg-Decode + Preview, wgpu-Renderloop, Tauri-Shell)
2. **Core-Editing**: Datenmodell, Timeline (Cut/Trim/Reorder), Preview-Playback
3. **Stabilisierung**: Gyro-Import, Stabilisierungs-Algorithmus, Lens-Profile
4. **Color & Audio**: LUTs/Farbkorrektur, Audiospur, OSD-Overlay
5. **AI intern**: Provider-Konfiguration, Tool-Schicht, Chat-Agent-Panel
6. **AI extern**: MCP-Server, CLI-Automation
7. **Export & Performance**: Render-Pipeline, Presets, Proxy-Workflow für 4K/60-Material
8. **Politur**: UI/UX-Feinschliff, Theming, Onboarding, Plugin-/Effekt-Schnittstelle für später

## 7. Offene Fragen / Entscheidungen

- Tauri vs. reines Rust-UI (iced) — Empfehlung: Tauri, wegen UI-Modernität
- Gyroflow-Code übernehmen (GPL-3.0-Bindung) vs. eigene Neuimplementierung (mehr Aufwand, lizenzfrei)
- Lizenzmodell der App selbst (Open Source? Falls ja: welche Lizenz, kompatibel mit ffmpeg-Build-Variante GPL/LGPL)
- Zielplattformen v1: macOS + Windows zuerst, Linux danach?

## 8. Nächste konkrete Schritte

1. Cargo-Workspace anlegen mit den Crates aus Abschnitt 2 (leere Skeletons)
2. Tech-Spike: Video in einem Tauri-Fenster per wgpu-Textur abspielen
3. Minimalen Command-Bus in `fpv-core` bauen (Add/Trim/Remove Clip) inkl. Undo/Redo
4. Erste `async-openai`-Anbindung mit konfigurierbarem Endpoint testen (einfacher Chat, noch ohne Tools)
