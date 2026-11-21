---
name: linnea
description: "Make videos written as code (motion graphics, explainers, reels, any frame you can draw) and edit footage into vertical reels with Linnea, a local Rust editor, through its MCP tools (or `linnea <tool> key=value` in a terminal). Use when the user hands over a talking-head recording, a long video or a link and says \"edit this\", \"make a reel\", \"cut clips from this\", \"add captions\", \"remove the pauses\", \"pick the best takes\", names a style (clean, editorial, meme, cinematic, launch, kinetic, pop, or a recipe such as Clean OS, RESULT, Caption News, Swiss Grid), or left review notes in the Linnea window."
---

# Linnea

One recording in, one reel out. The creative work is choosing takes and planning beats; the tools do the
rest the same way every time: cut math, 9:16 framing on the face, captions, sound levels, the render check.
Nothing on screen says anything the speaker did not say. A meme is a reaction: it lands ON or just after
the punchline, never before it.

The planning rules come from ghost-editor (MIT, kurbaitaev); its full skill, playbook and every recipe's
SYSTEM.md are in the library (`library/upstream`, `library/recipes`, `library/references`). The review loop
(notes on frames, the agent fixes and marks them) follows VideoHub (MIT).

## Films written as code (VideoHub's contract)

A film is a folder with `index.html` (a fixed-size canvas) and `film.js`, where `window.renderOut(t)` draws
second `t`. Every second of the picture is code you control: graphics, type, charts, pictures and footage
(seek a `<video>` and return a promise), so nothing is out of reach of an edit.

```
hub_new_film title=... width=1080 height=1920 dur=20   the folder, from the template
  edit film.js (Write/Edit)                            renderOut(t) a pure function of t; READY; VH_SCENES
  audio/mix.wav                                        the sound (optional); the preview plays it
hub_live film=<id>      loads it headless: READY, renderOut errors, console errors; then shows it in Linnea
hub_frame film=<id> t=4.2 | from=2 to=6                look at your own frames (Read the jpg) before handing over
STOP: the user watches live and leaves notes on frames
hub_notes film=<id>     read images.crop, then images.annotated; t, t1, frame, chunks, narration
fix film.js -> hub_frame the same t -> hub_note_update id=... status=fixed reply="what changed"
```

- The picture is a pure function of `t`: no `Date.now()`, no state carried between frames, `rand(seed)`
  instead of `Math.random()`. Then any frame in any order matches, and partial renders join seamlessly.
- Keep `live.dur` in `.videohub/project.json` true (`hub_project set={live:{dur:N}}`).
- Do NOT render. The user renders in the app when they are happy; a partial render redoes only the
  2-second chunks your fixed notes touch. `hub_render` refuses agents unless the user allowed it.
- The preview reloads by itself when a code file changes; the user does not need to do anything.
- Changing film.js? Keep scenes in `window.VH_SCENES` so notes and the timeline stay labelled.

### Edits without code (tweaks.json, VideoHub's keys; never delete them when editing the film)

- `hub_sound` / `hub_sound_apply` / `hub_sound_clip`: sound layers (stems voice, music, sfx, stems/*.wav, else
  the mix), levels in dB, mute, solo, removeMusic, ducking ranges, clips on the timeline (copied into
  vhaudio/). Apply rebuilds audio/mix.wav; the original stays as mix.vhorig.wav.
- Sounds you add to a film: put EACH effect, sting, riser and music piece on the timeline as its own clip
  (`hub_sound_clip film=… sound=whoosh at=2.4` for a library sound, `file=audio/pop.wav` for one in the film
  folder; `gain` in dB, `apply=true` rebuilds the mix). Never bake them into audio/mix.wav yourself: the user
  turns each clip up, down, off or solo in the Sound tab, and a sound inside the mix can only change with it.
  `hub_sound` lists the clips with their ids; `hub_sound clip={id, gain, mute, at} apply=true` changes one.
- `hub_2d`: 2D layers over the film (`items2d`: text, image, rect with a time range, px position and size,
  colours, fade). The linnea2d.js hook (in the template; `install=true` adds it) draws them after renderOut
  in the preview and in renders. Good for titles, labels and stickers the user wants to move by hand.
- Editable elements (KE, also in linnea2d.js): draw EVERY on-screen element of a film (titles, captions,
  cards, frames, characters, objects) through the template's `el(id, t, {label, x, y, t0, t1, text, color}, (p, tt) => ...)`
  or `KE.text(g, id, t, {text, x, y, size, color, bg, ...})`, with a stable id, reading p.text / p.color
  instead of literals. The user then clicks the element in Linnea and moves, resizes, turns, re-types,
  recolours, hides, retimes or keyframes it; edits live in tweaks.json `edits` (`hub_2d` returns and takes
  them) and never touch film.js. Keep those edits when you rewrite the code: same ids, same pivots.
  Plain fillText still works (id `txt:<text>`) but moves only the letters, not the box behind them.
- Puppets (the library of reusable drawings; check it before drawing anything from scratch):
  - `.ppet` = one drawing, `.rppet` = a rich puppet with named animation states (pencil: idle -> write).
    A file is JSON: {label, desc, tags, size: [w, h], params: {key: {type: color|number|text|bool, default,
    label}}, states: {name: {dur, loop, label}}, initial, assets: {key: data URL}, example, draw}. draw is
    the source of `function (g, p, t, s, u)` painting it in its own box 0..size, pure of its inputs:
    p = params, t = seconds since it appeared, s = {name, t, k (0..1), dur} the state playing,
    u = {ease, clamp, rand, img, W, H}.
  - `puppet_list` (query) -> `puppet_link film=… name=…` (copies it into the film's puppets/ and returns the
    line to use) -> in film.js `KE.puppet(g, 'pencil-1', t, {use: 'pencil', x, y (centre), scale, t0, t1,
    params, states: [{state: 'write', t0: 1.5}]})`. State times count from the puppet's t0.
  - Make one: whenever you draw a thing that could appear again (an object, a character, a device, an icon,
    a badge, a frame), write it as a puppet with `puppet_save` (params for what a user would change, states
    for its motions) and draw it with KE.puppet, instead of inline code. A film is then a scene of puppets
    the user can place, recolour and re-time by hand (Layers > Kukla kütüphanesi) and you can reuse anywhere.
  - `puppet_save film=… name=…` alone promotes the film's edited copy back to the library (version +1);
    `puppet_sync film=…` brings library updates into a film, keeping copies the film changed.
    `puppet_get as=js` gives it as an ES module for work outside Linnea; `puppet_get film=… name=…` returns the
    film's own copy. `puppet_save name=new from=old` starts a variant as a copy. Params also take
    `{type: 'select', options: [...]}`; `pivot: [x, y]` is the point KE.puppet's x, y place (default the centre).
    Talking characters: give the puppet `mouth` and `blink` number params (0..1); after `hub_lipsync film=…` the mouth
    follows the film's voice and the eyes blink by themselves (`KE.mouth(t)`, `KE.blink(t, seed)` in code). The `talker`
    puppet is a ready narrator. A state can blend in: `states: [{state, t0, blend: seconds}]`. A layer can follow a path:
    items2d `path: [[x, y], ...]` (centre points), `pathTurn`, `pathEase`. A puppet layer also takes `speed` (state
    motion ×), `fx` / `fy` (mirror) and `pkeys: [{t, p: {param: value}, ease}]` (numeric params animated over film time).
  - Ready rich puppets in the library: `stickman` (idle, walk, wave, cheer, jump, point, think; talks and blinks),
    `talker` (narrator), `check` (tick or cross drawn on), `cursor` (mouse pointer that clicks), `heart` (beat, burst),
    `confetti` (burst, rain; size the layer to the area), `timer` (countdown ring), `sparkle`, `pencil`, `bubble`.
  - Puppets a film keeps only in its own puppets/ folder are saved to the library by `hub_live` (the result
    lists them under `puppets_saved_to_library`); `puppet_harvest film=… all=true` also sends the ones you changed.
- Languages (one film, many versions): draw every on-screen text through KE.text / el() with a stable id (or
  plain fillText), then the user (or you) can translate it without touching film.js.
  `hub_lang_scan film=… action=texts` lists every text (element id, original, when it shows);
  `hub_lang film=… action=set lang=de name=Deutsch texts={id: "…"}` stores the translations (merged; keep
  each about as long as the original); `audio=audio/mix.de.wav` gives that language its own sound;
  `hub_lang_scan action=check lang=de` reports texts the translation pushed off the frame or made much wider
  (shorten those) and the ones still untranslated. In code, `window.VH_LANG` (or `KE.lang`) is the language
  being drawn ('' or null for the original), for anything that is not plain text. The user renders each
  language from the app; `hub_render lang=de` (when allowed) writes `<name>_de_vN.mp4`.
- Thumbnail Studio: `hub_thumbs`, `hub_thumb_new` (from a template), `hub_thumb_brief` (layers in words,
  positions, a prompt draft, layout.png), make the images, `hub_thumb_variant` each one, read
  `hub_thumb` notes for changes. Never put the title text in the image; the user adds it in Linnea.

## Footage reels: pipeline

```
doctor                      once per session: tools, models, fonts, each with its fix
import source=<file|url>    project, preview proxy, voice track            (job)
transcribe project=<id>     word times for the whole source                 (job)
words project=<id>          the sentence table = the take map, in ORIGINAL seconds
  long video:    clips_find -> clip_make clip=c1      (pauses trimmed, face tracked, becomes the active reel)
  one clean take: analyze -> autocut
  with retakes:  analyze -> retakes   (check `dropped` against `words`; fix takes with reel_set)
taste                       the user's standing corrections; read before planning
styles / recipe name=<id>   pick the look; a recipe returns its house look and planning document
style_apply style=<id> captions=<template>
beat_add / reel_set         plan the beats (below); every call returns warnings
build                       timeline, every caption with its placement, the sound table, warnings
snapshot t=<edit seconds>   one exact frame: LOOK at it (Read the file) before saying it is done
open                        show the reel in the Linnea window; the user watches and leaves notes
notes -> fix -> note_update status=fixed reply="..."
render mode=preview         then WAIT for the user's OK
render mode=final           only when the user says so; read the `qa` verdict and fix a FAIL
```

Long tools return `{job}`: call `job id=<id> wait=true` (again if it has not ended).

More footage tools: `export_subs` (SRT/VTT of the reel's captions), `render_batch` (every clip, in 9:16 /
1:1 / 4:5 / 16:9; only when asked), `reel_undo` (every save keeps the previous version; redo=true),
`style_save` (the reel's look as a brand style), `"voice": {"clean": true}` in a reel (denoise, low cut,
compression on the voice), `setup_tools` (download ffmpeg, yt-dlp, whisper, models on a fresh machine).

AI tools, each with a local and an API choice (nothing leaves the machine unless that provider is picked):
- Person matte (local GPU, ghost-editor's BiRefNet or the faster RVM): `behind` beats put words behind the
  speaker; `cutout` beats with fx bg-blur | selective | spotlight | bg-swap (bg "#hex" or a picture). `matte`
  makes a matte of any stretch. `analyze track=person` + `crop.track: "person"` follows the whole person;
  the default `face` also follows (a smoothed camera with a dead zone), `off` keeps the crop still.
- `translate lang=en` (captions.lang); `dub lang=en provider=elevenlabs-dubbing|elevenlabs|openai|piper`
  (audio.dub); `heygen_translate language="Spanish (Spain)"` (lip-synced, comes back as a new project).
- `broll_search` (Pexels, Pixabay), `broll_gen` (openai, gemini, local Automatic1111), `broll_auto`.
- `speed take=0 speed=1.5` or a ramp (`end_speed`, `from`, `to`); `eye_contact` (a local command, or Sieve).
- Clip scores come with a breakdown: hook, flow, value, and trend (trend only when the assistant picks).
- `publish_youtube` / `publish_tiktok` / `publish_instagram` are the user's step; agents are refused.

Layers and motion (films and footage reels alike): `reel_layers` (a reel) / `hub_2d` (a film) hold items2d:
text, image, rect, counter, marker (handwriting), arrow, phone (mockup with a notification), lower (lower
third), progress, ellipse, puppet. Each can carry `keys: [{t, x, y, w, h, rot, scale, opacity, ease}]` (keyframes; a key
may hold only some properties) and a `parent` (a rig: the child follows the parent's motion). The look: `anim` (way
in) and `animOut` (way out): none|fade|slide-up|slide-down|slide-left|slide-right|pop|zoom|spin|blur|wipe over `fade`
seconds; `shadow` (colour) + `shadowBlur/X/Y`, `blur`, `blend` (canvas composite: multiply, screen, overlay…), `fx`/`fy`,
`border` + `borderWidth`, `bg2` + `gradAngle` (gradient fill). Text: `font, size, weight, italic, align, spacing,
lineHeight, upper, stroke, strokeWidth, typing: 'chars'|'words'` (typed out over `reveal` s). Image: `fit:
cover|contain`, `radius`. `label` names a layer, `locked` keeps it from being dragged in the app. Reel layer times are EDIT seconds; the user drags
and edits them in the editor. Any custom animation can go into the reel's layer page film.js (the folder
`reel_layers` returns), drawn transparent over the reel.

Cues and clips: `signals` (voice energy, pitch movement, picture motion per second), `vision_score` (a vision
model rates one frame a second for expressions, laughter, gestures, graphics); `clips_find` then ranks with an
"emotion" part. `clip_make prepend_hook=true` or `hook_add` puts the strongest 2-3.5 s first with a title and
a whoosh. `speakers` (podcasts, interviews) + `crop.mode: "speaker"` follows whoever talks and splits the
frame top/bottom when two react together.

## Rules

- Every time in a reel is an ORIGINAL-recording second from `words`, except `"end"`, `"outro+N"`,
  `"hold:<take>+<s>"` and `"edit:<s>"`. A time inside cut material snaps to the next kept moment.
- Cut on word times (`words format=words`), never inside a word: `a` = first word - 0.10..0.15,
  `b` = last word + 0.20..0.30. Before a meme, end the take 0.6-1 s later or give it a `hold`.
- Pick the take of each sentence that is fluent and later. Keep one-breath runs as one take. Aim for 45-75 s
  for a talking head; 20-60 s for a clip.
- Hard cuts are the default. A reframe (`zoom.snaps: [[t, 1.1]]`) at a cut or sentence start hides a jump
  cut; at most three punch-ins per reel; 4-6 slow `zoom.pushes` at most.
- Walk the transcript sentence by sentence: what did the speaker just make the viewer imagine? Show it on
  the word. Leave some sentences bare. One card in the band and one thing in the reaction slot at a time.
- Budgets for a talking head: full-screen B-roll under 25 %, at most 3 inserts a minute, audible sounds
  0.4-0.9 per 10 s, memes 1-3 a minute and never the same sound twice.
- The speaker's face keeps its as-shot colour. `look.grade` is ignored unless `look.asked` holds the user's
  own words asking for a grade. Grain and vignette are fine.
- Sound levels are set against the voice by role (ui -14, whoosh -12, impact -8, meme -3 dB). Never
  hand-tune volume and never lift the finished mix. A meme sound never sits on a word.
- No em dashes in on-screen text. Light accents need dark text on them (`brand.onAccent`).
- Fix every warning `build` prints, or say why it stays.
- When the user corrects an edit decision, fix the reel AND record it: `taste add="<their words>"`.

## The reel (reel_set patches it; null deletes a key, arrays replace)

```jsonc
{ "title": "...", "width": 1080, "height": 1920, "fps": 30, "platform": "instagram",   // tiktok | shorts | all
  "style": "clean", "takes": [{ "a": 6.58, "b": 10.40, "hold": 0.8 }], "outro": 0,
  "crop": { "mode": "auto" },                 // auto (follow the face) | center | manual + x,y 0..1 | fit (blurred bars)
  "zoom": { "origin": "50% 29%", "base": 1, "snaps": [[10.76, 1.1]], "pushes": [{ "at": 19.8, "z": 1.1, "up": 0.4, "until": 22.4, "down": 0.2 }] },
  "captions": { "style": "hype", "group": 3, "size": 64, "y": 1200, "upper": false, "accent": "#FFD23F",
                "fixes": { "Jithub": "GitHub" }, "phrases": { "many chat": "ManyChat" }, "keywords": ["free", "$20"] },
  "brand": { "accent": "#FFD23F", "ink": "#111111", "font": "Montserrat" },
  "look": { "grain": 0.07, "vignette": 0.45 }, "music": { "src": "music/bed.mp3", "db": -20 },
  "sfxProfile": "standard",                   // restrained | standard | rich
  "beats": [], "sfx": [{ "at": 24.4, "id": "vinyl-scratch" }] }
```

Caption templates: `clean` `house` `pill` `editorial` `hype` `explosive` `line-focus` `neo-minimal`
`karaoke-fill` `highlighter` `mini` `chest` `none`. Any template field can be overridden in `captions`
(font, weight, size, case, mode phrase|accumulate|karaoke|lines2, active color|pop|box|grow, entry
punch|fade|blur, keyword color|box|marker|underline|serif, bg pill|card|bar, align, x, y, maxw, group).
Captions sit below the chin, else above the head, inside the platform's safe area, and step around cards.

## Beats (`at`, `to`, optional `sfx`: an id, a list, or "none")

| type | fields | use when the speaker says |
|---|---|---|
| `big` | `text`, `sub`, `count` | a number, a duration, a claim |
| `chips` | `items: [{text, at, bg, fg}]` | a list of names, one chip per spoken word |
| `strike` | `text`, `strikeAt` | "not X", a myth they dismiss |
| `quote` | `header`, `lines: [{text, at, big}]` | what someone told them |
| `list` | `lines: [{text, at}]`, `stamp`, `stampAt` | a list they then dismiss |
| `title` | `text` or `words: [{text, at}]`, `y`, `size`, `font`, `anim: pop\|blur` | giant words above the head |
| `emoji` | `emoji` (glyph) or `src` (picture), `size` | a feeling |
| `logo` / `meme` | `src` (file in the project), `w`, `x`, `y`, `audio` | a product; a punchline |
| `broll` | `src`, `in`, `mode: "split"`, `h` | the thing they describe, on the word |
| `endcard` | `title`, `line`, `url` (at `"outro+0"`, with `outro` >= 2) | the close |
| `textfx` | `kind: hook\|headline\|spring`, `key`, `word`, `sub`, `lines` (`*marked*`), `mark: {style, at}`, `bg` | the hook (first 1.5-3 s), the line that carries the idea |
| `scene` | `kind: card\|word\|sentence\|kinetic\|stats\|ui\|device\|image`, `bg`, `lines`, `pill`, `hero`, `badges`, `prompt`, `in`, `captions` | a full-screen graphic over the continuing voice |

Not in this build (the build says so): a person matte (`behind`, `cutout`, the stan `panel` cut-out), shader
backgrounds (drawn flat), `drum` / `scramble` / `flipstack` (drawn as a headline), `fly3d`. The recipe packs
carry each recipe's type, colours and caption system; their bespoke animated scenes are listed per pack in
`_grammar` as not native. Say so when the user asks for one of those, and propose the nearest native beat.
