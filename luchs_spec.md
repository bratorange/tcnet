# LUCHS — Annotator
## Implementation Specification

---

## Table of Contents

1. [Product Overview](#1-product-overview)
2. [Layout Overview](#2-layout-overview)
3. [GUI Elements — Detailed Description](#3-gui-elements--detailed-description)
   - 3.1 Top Bar
   - 3.2 Overview Deck Cards
   - 3.3 Melodic/Percussive Gradient Strip
   - 3.4 Phrase Bar
   - 3.5 Analysis Progress Indicator
   - 3.6 Needle View Lanes
   - 3.7 Beat Grid (Needle Lanes)
   - 3.8 PitchMelodia Contour
   - 3.9 Phase Strip (Needle Lanes)
   - 3.10 Status Bar
   - 3.11 Segment Override Modal
4. [User Manual](#4-user-manual)
5. [Data Architecture & Background Processing](#5-data-architecture--background-processing)
6. [Open Questions & Missing Information](#6-open-questions--missing-information)

---

## 1. Product Overview

Luchs is a standalone VJ annotation client that connects to a DJ setup via the **TCNet** protocol. It monitors up to four CDJ players simultaneously and provides real-time phrase-level structural analysis of the currently loaded tracks. Its primary audience is a **VJ**, who needs to anticipate the structural shape of live music — drops, breakdowns, verses, instrumental sections — in order to synchronise visual output to the music.

It is a passive listener and analysis display, which forwards information to lighting software.

The application runs a phrase segmentation model in the background. Analysis results are progressively streamed into the UI as they become available — sections that are not yet analysed are displayed with a shimmer placeholder. Completed results persist for the lifetime of the loaded track.

---

## 2. Layout Overview

The interface is divided into four vertical sections, read top to bottom:

```
┌─────────────────────────────────────────────────────────┐
│  TOP BAR — connection status, on-air / next indicators  │
├──────────────────────┬──────────────────────────────────┤
│  OVERVIEW DECK 1     │  OVERVIEW DECK 2                 │
│  (CDJ 1)             │  (CDJ 2)                         │
├──────────────────────┼──────────────────────────────────┤
│  OVERVIEW DECK 3     │  OVERVIEW DECK 4                 │
│  (CDJ 3)             │  (CDJ 4)                         │
├─────────────────────────────────────────────────────────┤
│  NEEDLE LANE — CDJ 1  (8-bar scrolling window)          │
│  NEEDLE LANE — CDJ 2                                    │
│  NEEDLE LANE — CDJ 3                                    │
│  NEEDLE LANE — CDJ 4                                    │
├─────────────────────────────────────────────────────────┤
│  STATUS BAR — global timing, phrase, transition info    │
└─────────────────────────────────────────────────────────┘
```

**Overview section (2×2 grid):** Shows the full track loaded into each CDJ at minimum zoom — the entire track fits into the waveform width. This gives the VJ a macro view of the track's energy shape and phrase structure.

**Needle section (4 stacked lanes):** Shows a zoomed, scrolling 8-bar window for each CDJ with a fixed playhead ("needle") anchored at approximately 22% from the left edge. The waveform scrolls rightward under the needle as the track progresses. This gives the VJ a real-time view of what is happening right now and what is coming up within the next 8 bars.

**Deck state colour coding:**
| State | Top border | Label | Opacity |
|---|---|---|---|
| On air (playing) | Red `#e83` | `▶ ON AIR` | 100% |
| Next (cued, will play) | Blue `#26a` | `▷ NEXT` | 100% |
| Idle (paused, cue-setting) | None | `‖ CUE` or `‖` | 38% |

---

## 3. GUI Elements — Detailed Description

### 3.1 Top Bar

**Visual:** Single horizontal bar at the top of the application. Dark background (`#111`), 0.5px bottom border.

**Contents:**
- **Logo / app name** (`LUCHS — annotator`): Static label, left-aligned.
- **TCNet connection indicator:** A pulsing green dot (`●`) followed by the TCNet IP address and player count. The dot animates between full and 40% opacity on a 2-second cycle to indicate an active connection. If connection is lost `[PLACEHOLDER: disconnection state]` the dot turns amber and the label changes to `TCNet — reconnecting`.
- **ON AIR badge:** Red pill badge (`▶ ON AIR: DECK-X`). Always shows which deck is the current master output. Updates whenever the on-air deck changes.
- **NEXT badge:** Blue pill badge (`▷ NEXT: DECK-X`). Shows which deck the DJ has prepared as the upcoming track. `[PLACEHOLDER: logic for determining "next" from TCNet state]`.

**Functional role:** Provides persistent connection and routing status so the VJ always knows which deck to watch.

---

### 3.2 Overview Deck Cards

There are four deck cards arranged in a 2×2 grid. Each card represents one deck player.

**State-dependent rendering:**
- Cards for idle/paused decks render at 38% opacity (both the card content and the corresponding needle lane below).
- The on-air card has a 2px red left border and a 2px red top strip.
- The next card has a 2px blue left border and a 2px blue top strip.

**Contents per card:**

#### Deck ID badge
Small coloured pill in the top-left of the header row. Values: `DECK 1 ▶ ON AIR` (red), `DECK 3 ▷ NEXT` (blue), `DECK 2 ‖ CUE` or `DECK 4 ‖` (dark grey, for idle).

#### Track title
Truncated single-line text label showing the track name as reported over TCNet.

#### BPM display
Right-aligned, amber coloured. Shows current BPM to two decimal places. A smaller grey sub-label shows the tempo adjustment percentage (e.g. `+0.0%` or `-1.2%`). `[PLACEHOLDER: source BPM vs adjusted BPM distinction]`

#### Metadata row
A compact horizontal row below the header showing:
- **TIME** — Current playback position as `mm:ss`. Updates live for playing decks.
- **GRID** — Time signature, e.g. `4/4`. `[Detected from beat grid data]`
- **CUES** — A row of small coloured dots, one per hot cue point set on the Deck. Colours correspond to the CDJ's own cue colour assignments.

#### Waveform (overview)
A full-track waveform rendered. Each vertical bar represents a short time window; bar height encodes Pitch Melodia, which is also pre analysed. The waveform is **phrase-tinted**: each column is coloured according to the phrase type assigned to that time position:
- Instrumental: amber `rgba(255,160,0,α)`
- Chorus / drop: cyan `rgba(60,180,255,α)`
- Verse: light blue `rgba(80,200,255,α)`
- Break: violet `rgba(200,100,220,α)`
- Not yet analysed: grey `rgba(80,80,80,α)`

The portion of the waveform to the left of the playhead position renders at higher opacity (played), the portion to the right at lower opacity (upcoming). A thin red vertical line marks the current playback position.

Beat grid lines are **not shown** in overview decks — at minimum zoom they would add visual noise without useful spatial information.

---

### 3.3 Melodic / Percussive Gradient Strip

**Visual:** An 8px tall strip directly below the waveform in each overview deck card. No text labels.

**What it shows:** A continuous per-column colour gradient encoding how melodic vs percussive the audio content is at each point in time. Uses the **Magma** colormap (matplotlib):
I.e.
- Deep purple → `t = 0.0` → fully percussive
- Teal/green → `t = 0.5` → mixed
- Bright yellow → `t = 1.0` → fully melodic

Each pixel column maps to a time position in the track. The colour value is derived from a melodic ratio `m ∈ [0, 1]` computed for that time position. `[PLACEHOLDER: melodic ratio computation — likely harmonic-to-percussion ratio from source separation, spectral flatness, or dedicated M/P classifier]`

Played portion renders at 90% opacity; unplayed at 40%. A small red tick marks the current playhead position, consistent with the waveform above.

**Functional role:** At a glance the VJ can identify whether an upcoming section will offer melodic content to react to (synths, leads, pads) or purely rhythmic energy. Techno tracks typically show long purple regions broken by short yellow patches during melodic phrase segments.

---

### 3.4 Phrase Bar

**Visual:** A 13px tall horizontal bar divided into labelled colour-coded segments, one per phrase section. Sits between the M/P strip and the analysis indicator. Segments are separated by 1px gaps.

**Phrase types and colours:**

| Type | Background | Text colour | Meaning |
|---|---|---|---|
| `inst` | `#3a2800` amber-dark | `#fa0` amber | Instrumental / non-vocal section |
| `chorus` | `#003a28` teal-dark | `#4af` cyan | Drop or chorus (high energy, repeating) |
| `verse` | `#00283a` blue-dark | `#6cf` light blue | Verse or mid-energy section with melodic content |
| `break` | `#2a0030` purple-dark | `#d6a` violet | Breakdown, bridge, or tension section |
| Computing | Animated diagonal shimmer | — | Analysis not yet complete for this time window |

**Segment width** is proportional to the number of bars in that segment relative to the total track length. Segment bounders therefore align vertically with the waveform.

**Text label** is shown inside the segment if the segment is wide enough (≥ 6 proportional units); otherwise the segment is unlabelled.

**Functional role:** The phrase bar is the primary structural annotation display. It gives the VJ an at-a-glance map of the entire track's arrangement, so they can plan visual scene changes around structural boundaries.

---

### 3.5 Analysis Progress Indicator

**Visual:** A compact row containing a label, a thin progress track, and a percentage readout. Sits at the bottom of each overview deck card.

**States:**
- **Analysis in progress (live):** Label reads `ANALYSING...`. Progress fill is cyan, percentage updates live.
- **Queued:** Label reads `ANALYSIS ⟳ queued`. Fill is amber.
- **Complete:** The entire row is **hidden** — no visual element is shown. Removing this indicator once analysis is done reduces visual noise on tracks that have been loaded for more than a few seconds.

analysis queue management — on-air first, then next, then idle decks

---

### 3.6 Needle View Lanes

Four stacked horizontal lanes in the lower half of the interface, one per Deck.

**Visual:** Each lane is 64px tall with a dark background. The left 36px is a label column; the remainder is a canvas rendering area.

**Label column:** CDJ index and BPM. Colour matches deck state: red for on-air, blue for next, grey for idle.

**Idle lane dimming:** Idle deck lanes render at 32% opacity — slightly more aggressive than the overview cards — because the needle view is primarily useful for decks that are actively progressing.

#### Scrolling waveform
The waveform in the needle lane is not static — it scrolls. The window displays exactly **8 bars** worth of audio at all times. The needle is fixed at 22% from the left edge. As the track plays, the canvas redraws each frame so that the current playback position always sits under the needle. Content to the left of the needle (already played) renders at higher opacity; content to the right (upcoming) at lower opacity.

The waveform uses the same phrase-tint colour scheme as the overview waveform.

Here the BigWaveformData is requested from the TCNet brigde.

Since time packets wont come often enough, the position must be constantly calculated from the playback position and speed. TCNet time packets will have higher authority.

#### Needle / playhead
- A fixed vertical line at 22% of the lane width.
- On-air deck: red (`#f44`). Next deck: blue (`#26a`). Idle: red but lane is dimmed.

---

### 3.7 Beat Grid (Needle Lanes Only)

Beat grid lines are drawn directly onto the needle lane canvas, behind the waveform. Three levels of visual weight:

| Beat position | Line | Height | Opacity |
|---|---|---|---|
| Beat 1 (downbeat) | 1.5px white, full height | 100% | 55% |
| Beat 3 (upbeat) | 1px white, 55% height centred | — | 22% |
| Beats 2 & 4 | 0.5px white, 28% height centred | — | 10% |

Bar numbers are rendered in small type (7px monospace) just above the centreline at each downbeat.

Beat positions are calculated from: `beat_px = (window_duration_sec / beat_duration_sec) * lane_width_px`

Data from TCNet beat grid packet.

---

### 3.8 PitchMelodia Contour

**Visual:** An amber/yellow line drawn over the needle lane waveform. A very faint filled area connects the contour to the waveform midline.

**What it shows:** The fundamental frequency (F0) pitch contour of the melodic content at each point in the 8-bar window, estimated by a PitchMelodia-style algorithm. Pitch value is normalised to the range [20, 100] (relative units) and mapped vertically within a 52% height zone centred on the lane.
This calculation is part of the analysis.

Only regions where pitched material is detected have a contour line — silent sections, purely percussive sections, and not-yet-analysed regions show no line. The more melodic content is detected, the more pronounced the contour line becomes.

**Rendering:** The line is drawn as a continuous polyline. Where the pitch is unvoiced (zero), the line breaks. A faint filled polygon connects the contour to the midline for depth.

**Functional role:** The pitch contour lets the VJ see the melodic shape of an upcoming section before it is heard — whether it builds, stays static, or falls. Useful for keying visual palette changes or effects to melodic motion.

---

### 3.9 Phrase Strip

**Visual:** An 8px tall strip anchored to the bottom of each needle lane. Divided into proportional segments matching the phrase bar in the overview card, using the same colour coding (darker variants of the phrase type colours for better legibility against the waveform).

**Functional role:** Shows phrase boundaries and types in the zoomed needle view without obscuring the waveform. The strip scrolls with the waveform — the segment visible under the needle corresponds to the current phrase.

---

### 3.10 Status Bar

**Visual:** A single dark horizontal bar at the very bottom of the application.

**Contents (left to right):**
- **MASTER** — master BPM (taken from the on-air CDJ). `[PLACEHOLDER: master clock source selection from TCNet]`
- **BAR** — current bar number / total bars in track. `[PLACEHOLDER: bar counting from beat grid + playback position]`
- **PHRASE** — current phrase type (coloured to match phrase colour scheme) and bars remaining in the current phrase, e.g. `CHORUS (CDJ1) · 8 bars left`.
- **TRANSITION** — estimated bars until the next likely transition (e.g. `▲ in ~32 bars`). next phrase boundary.

---

## 4. User Manual

### First launch

1. Ensure the CDJs and PhraseTrack are on the same local network as the TCNet master device (typically a TCNet bridge).
2. Launch LUCHS. The top bar will show a pulsing green dot and the detected TCNet IP once a connection is established.
3. Load tracks into the CDJs as normal. PhraseTrack will detect loaded tracks automatically and begin analysis.

### Reading the interface

**Before the set starts:** Check that all four decks show completed phrase bars (no shimmer segments). If a track was loaded very recently, the analysis progress indicator will show a percentage. Tracks loaded during the set will analyse in the background — partially analysed tracks are usable; shimmer regions simply mean "trust your ears here".

**During the set:** Focus on the needle lanes. The on-air deck (red needle) shows what is happening now and what is coming in the next 8 bars. The next deck (blue needle) shows what the DJ is about to play. Use the phrase strip at the bottom of each needle lane to identify upcoming structural boundaries.

**Reading the M/P strip:** Yellow/green sections in the gradient strip indicate melodic content — good moments for detailed or reactive visuals. Purple/dark sections are percussive — suitable for rhythm-driven or beat-synced visuals.

**Reading the pitch contour:** In the needle lanes, the amber line shows the pitch curve of any melodic content in the window. A rising contour suggests a build; a flat contour suggests a sustained pad or static synth hook.

**Anticipating transitions:** Watch the status bar's TRANSITION field. The `▲ in ~N bars` readout gives an estimated window. Cross-reference with the overview phrase bar of the NEXT deck to understand what the incoming track's opening section looks like.

---

### Track identification and audio retrieval

`[PLACEHOLDER: how audio is obtained for analysis — options include: (a) track file path from rekordbox database via Pro DJ Link file transfer, (b) direct audio capture from mixer output, (c) pre-analysis of the rekordbox library off-line. Preferred approach TBD.]`

### Analysis pipeline

The analysis pipeline runs asynchronously per loaded track. It should expose a streaming results interface so the UI can display partial results as they are computed. Jobs are queued and prioritised:

1. On-air deck — highest priority
2. NEXT deck — second priority
3. Idle decks — background priority

Each job produces the following outputs over time:

| Output | Description | Update cadence |
|---|---|---|
| Phrase segmentation | Array of `{start_bar, end_bar, type, confidence}` | As segments complete |
| Melodic ratio curve | Array of `{time_sec, melodic_ratio}` | `[PLACEHOLDER: frame rate]` |
| PitchMelodia contour | Array of `{time_sec, f0_hz, voiced}` | `[PLACEHOLDER: frame rate]` |
| Beat grid | Array of `{beat_index, time_sec, bar_number}` | Once, on grid detection |

`[PLACEHOLDER: phrase segmentation model — architecture, input features, output format, confidence scoring, model weights location, inference runtime]`

`[PLACEHOLDER: melodic/percussive separation method — e.g. HPSS (harmonic-percussive source separation), dedicated classifier, or ratio of spectral features]`

`[PLACEHOLDER: PitchMelodia implementation — library, frame hop size, voicing threshold]`

### State management

The UI maintains an in-memory state as defined by TCNet and the analysis pipeline. Analysis results are merged into the state, and be cached in cache directory.

---


1. **Audio source** *(analysis pipeline)*
   All tracks will be mirrored from the mixer and availabe in a media folder. (usb folder)

2 **Phrase model output format** *(UI data contract)*
   What is the exact output schema of the phrase segmentation model? Are phrases in bars, seconds, or sample frames? Is confidence a scalar or a per-class probability vector?

Analysis: 
- A phrase analysis model is used to segment audio tracks into phrases. For the begining this will just be the allin1 python script.
  - The phrase bar is computed from the phrase segmentation model output.
- BeatGrid Information is retrieved from the TCNet beat grid packet.
- Waveform data is requested from the TCNet brigde bigwaveform and  smallwaveform packets.
- Melodic/Percussive Gradient is computed from the audio track. Have a look at how this is done in librosa.
- The PitchMelodia contour is computed from the audio track. Reimplement the PitchMelodia algorithm: https://essentia.upf.edu/reference/std_PitchMelodia.html

- **"Next deck" detection** *(top bar, status bar)*
   A heuristic is used to detect which Deck is about to play next. Often times djs will load a new track into the deck that is not playing.
and allign cue points with the current song. Also the next song will start to play and be faded in. This indicates, that it will be the next deck.

**Transition prediction** *(status bar)*
   The `~N bars to transition` value derived from phrase boundaries. Potentially also one from the deck which is gone play next.


**Multiple simultaneous on-air decks** *(top bar, deck state)*
   During a blend/crossfade, two decks may technically be on air. How should this be displayed? The crossfader value is taken as deciding factor here.

The entire UI is created with eframe/ egui, to keep it portable and performant.


**No track loaded state** — What should a deck card show when no track is loaded into that CDJ? -> Nothing

# Using the live track data
- A small settings menu is accessible from the top right, where osc endpoints can be configured.
- The app should send data live to the osc endpoints configured in the settings menu.
- There should be an endpoint configurable for the phrase with integers encoding the phrase type. It should only send
on segment borders, but also at a transition from i.e. "verse" to "verse".
- An osc beat index shall be send on every beat in order to syncronize the bpm of a other software