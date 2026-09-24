# SoundFonts

Audio preview uses one instrument-specific SoundFont per instrument. These files
are redistributed with the app; all are under licenses that permit redistribution.
They are resolved by filename (see `SPECIFIC_FONTS` in
`preview_synth/src/lib.rs`).

| Instrument | File | Font | License |
|---|---|---|---|
| Guqin 古琴 | `OLPC_Guzheng.sf2` | OLPC Guzheng | CC BY 3.0 |
| Pipa 琵琶 | `MFA_Pipa.sf2` | MFA Pipa | CC BY 3.0 |
| Erhu 二胡 | `FS_Erhu_v2.sf2` | FS Erhu v2 | CC BY 3.0 |
| Konghou 箜篌 | `ConcertHarp.sf2` | FreePats Concert Harp | CC0 1.0 |
| Fangxiang 方響 | `Xylophone.sf2` | FreePats Xylophone | CC0 1.0 |

If an instrument-specific file is missing, the app falls back to a General MIDI
bank (`FluidR3_GM.sf2` / `FluidR3_GS.sf2`, MIT) placed in
`~/.local/share/where-winds-meet-player/soundfonts/`. No GM bank is bundled.

## Attribution

### CC BY 3.0 (attribution required)

The three fonts below were downloaded from [Musical Artifacts](https://musical-artifacts.com/)
and are unmodified. CC BY 3.0 <https://creativecommons.org/licenses/by/3.0/>.

- **OLPC Guzheng** — by Mr Fuzzywump (sample thanks to Lisa Lim).
  Source: <https://musical-artifacts.com/artifacts/3077>
- **MFA Pipa** — by Mr Fuzzywump.
  Source: <https://musical-artifacts.com/artifacts/3060>
- **FS Erhu v2** — by Mr Fuzzywump; the underlying samples originate from the
  FlameStudios free SoundFont collection, released under the GNU GPL.
  Source: <https://musical-artifacts.com/artifacts/3078>

### CC0 1.0 (public domain)

The two fonts below are from the [FreePats project](https://freepats.zenvoid.org/)
and are dedicated to the public domain (CC0 1.0,
<https://creativecommons.org/publicdomain/zero/1.0/>). Both are assembled from
the Versilian Community Sample Library (VCSL), a CC0 library by Versilian
Studios. Samples were edited; the files here are distributed unmodified.

- **FreePats Concert Harp** — <https://freepats.zenvoid.org/OrchestralStrings/harp.html>
- **FreePats Xylophone** — <https://freepats.zenvoid.org/ChromaticPercussion/xylophone.html>

## Not bundled

These are present on some development setups but are **not** redistributed because
their licenses do not allow it (see `.gitignore`):

- `ACCURATE_SF2_AiX_CTX800.SF2` — all rights reserved; embeds samples from
  third parties. It was previously used for Konghou (32/46) and Fangxiang (32/98);
  the CC0 FreePats fonts above replace it.
- `DSK Asian DreamZ.SF2` — DSK freeware license; no derivative redistribution.
  Only an optional fallback for Guqin/Pipa/Erhu, which already have dedicated fonts.
