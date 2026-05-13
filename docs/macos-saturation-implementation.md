# macOS Saturation Implementation

This document records the implementation path for macOS saturation adjustment,
why the current method was selected, and what constraints remain. It is intended
for collaborators who need to understand the native color pipeline rather than
reverse-engineer the discussion history.

## Goal

The product has three visual channels:

- `saturation`: reduce screen saturation toward grayscale.
- `color_temp`: reduce color temperature toward warmer tones.
- `brightness`: reduce screen brightness.

On Windows these can be represented together through one color transform path.
On macOS the existing warmth and brightness implementation used Core Graphics
gamma tables, while saturation required a different route.

The macOS saturation goal was:

- No screen recording permission.
- No mirrored low-resolution screen overlay.
- Smooth enough for preview and gradual timer transitions.
- Stable reset on app shutdown, force reset, and mode shutdown.
- Keep Windows extensibility unaffected.

## Routes Evaluated

### CABackdropLayer and CAFilter

This route creates a fullscreen transparent window and inserts a private
`CABackdropLayer`/`CAFilter` stack to filter pixels behind the app.

It was rejected because the effect was not stable in practice. It could produce
a short saturation change, but the system flattened or invalidated the layer
tree after a short time, so the effect disappeared after one or two seconds.
That makes it unsuitable for long-running sessions.

### ScreenCaptureKit Mirror Overlay

This route captures the screen, applies a filter to the captured frames, and
renders them back in a fullscreen overlay.

It was rejected because it requires Screen Recording/TCC permission and produced
poor user experience in testing: low-quality mirrored output, permission errors,
and failure modes severe enough to black out the screen until the app was
forced to quit.

### CoreGraphics Gamma Table

Warmth and brightness currently use Core Graphics gamma tables through
`CGSetDisplayTransferByTable`. Gamma tables are good for per-channel curves:

```text
R' = f(R)
G' = f(G)
B' = f(B)
```

That works for brightness and color temperature because both can be modeled as
per-channel scaling or curve changes. Saturation is different: desaturation
mixes channels based on luma, so each output channel depends on all input
channels:

```text
R' = f(R, G, B)
G' = f(R, G, B)
B' = f(R, G, B)
```

Gamma tables cannot express that cross-channel RGB mixing.

### ColorSync ICC Display Profile

The accepted route uses ColorSync display ICC profiles. Instead of capturing or
overlaying the screen, the app generates a temporary display profile and asks
macOS to use it for the display through ColorSync.

This route was accepted because it satisfies the main product constraints:

- No screen recording permission.
- No mirror overlay.
- Smooth changes in testing, including gradual `1.0 -> 0.4` transitions.
- Low runtime overhead after the profile is applied.
- Reset is possible by clearing the custom ColorSync profile.

## Current Method

The implementation lives in:

- `src-tauri/src/renderers/mac_colorsync_saturation_filter.rs`
- `src-tauri/src/renderers/mac_colorsync_saturation_filter.c`
- `src-tauri/src/renderers/macos.rs`

The Rust wrapper owns renderer state:

- Whether saturation is currently switched on.
- The last applied percentage bucket.
- The temporary profile directory.
- A captured baseline ICC profile path.

The C file performs the ColorSync and ICC work.

## Implementation Flow

### Startup

When saturation is enabled, the renderer:

1. Creates a temporary profile directory under the system temp directory.
2. Captures the current display profile using `ColorSyncProfileCreateWithDisplayID`.
3. Writes that profile to `baseline.icc`.
4. Marks the saturation renderer as active.

Capturing `baseline.icc` is important. Generated profiles must always be based
on the original display profile, not on the currently modified profile.
Generating from the current modified profile caused a bug where preview could
move from full color to grayscale but could not recover cleanly from grayscale
back to color.

### Render

For each saturation value:

1. Clamp the amount to `0.2..=1.0`.
2. Quantize it to a percentage bucket, for example `0.40 -> 040`.
3. Skip if the bucket is unchanged.
4. Load `baseline.icc`.
5. Copy the profile into a mutable ColorSync profile.
6. Read the `rXYZ`, `gXYZ`, and `bXYZ` tags from the baseline profile.
7. Build the display primary matrix:

```text
M = [rXYZ gXYZ bXYZ]
```

8. Build a Rec.709 luma saturation matrix:

```text
luma = [0.2126, 0.7152, 0.0722]
S[row][col] = luma[col] * (1 - saturation) + identity[row][col] * saturation
```

9. Invert that saturation matrix.
10. Multiply the display primary matrix by the inverse saturation matrix:

```text
M' = M * inverse(S)
```

11. Write `M'` back into the `rXYZ`, `gXYZ`, and `bXYZ` tags.
12. Verify the profile with `ColorSyncProfileVerify`.
13. Write the generated ICC file to disk.
14. Apply it through `ColorSyncDeviceSetCustomProfiles`.

The inverse is intentional. ColorSync uses the display profile as part of the
display color conversion path; empirically, writing the forward saturation
matrix produced the wrong direction and made colors more saturated. The accepted
implementation uses the inverse matrix and was validated visually.

The bucket check is also intentional. The engine runs at roughly 60 Hz, and the
channel curve can produce tiny floating-point changes on every frame. Applying a
new ColorSync display profile for every frame is too expensive, especially on
battery power, because it asks macOS to reconfigure global display color state.

The renderer therefore treats the percentage bucket as the real application
unit:

```text
bucket = round(saturation * 100)
applied_saturation = bucket / 100
```

If `last_bucket == bucket`, the renderer skips profile generation and
`ColorSyncDeviceSetCustomProfiles`, even if the raw floating-point saturation
value changed. This caps a full `1.0 -> 0.2` transition at roughly 80 profile
applications instead of allowing near-60-Hz profile changes throughout the
transition.

### Shutdown and Reset

When saturation is disabled or the renderer shuts down, it calls
`ColorSyncDeviceSetCustomProfiles` with `kCFNull` for the display default
profile. This clears the app's custom display profile and returns the display to
the system/default ColorSync profile.

Additional reset protection exists on app exit so that the display profile is
restored even if the normal renderer shutdown path is not the last code path to
run.

## Why Saturation Is Temporarily Mutually Exclusive on macOS

The accepted saturation route and the existing warmth/brightness route currently
use different macOS display APIs:

- Saturation uses ColorSync ICC display profiles.
- Warmth and brightness use Core Graphics gamma tables.

Both APIs modify global display color state. macOS does not provide this app a
single stable atomic transaction that combines "custom ColorSync profile" and
"custom gamma table" into one coherent update.

In practice, combining them caused:

- Saturation plus warmth: lag.
- Saturation plus brightness: flicker.

The current app therefore enforces mutual exclusion on macOS only:

- Turning on saturation turns off warmth and brightness.
- Turning on warmth or brightness turns off saturation.
- A temporary UI log explains the automatic change only when it happens.

Windows does not enforce this constraint because the Windows implementation can
represent the visual channels in one color-transform path.

## Current Limitations

The current ColorSync implementation modifies only the ICC profile matrix tags
`rXYZ`, `gXYZ`, and `bXYZ`. It does not generate a full ICC 3D LUT.

That is enough for the accepted saturation implementation, but it means the app
is not yet using a general-purpose `RGB -> RGB` ColorSync transform.

The lower saturation bound is currently `0.2`. Values below that become more
numerically risky because the saturation matrix inversion becomes increasingly
aggressive and profile verification/display behavior becomes less predictable.

## Possible Next Step

The cleanest future direction is to move all macOS visual channels into one
ColorSync profile path. Conceptually:

```text
[R', G', B'] = F(R, G, B)
```

Brightness and warmth can be folded into the same matrix math as saturation:

- Brightness as global RGB scale.
- Warmth as per-channel RGB scale.
- Saturation as cross-channel luma mixing.

If that unified matrix probe works, macOS would no longer need mutual exclusion
between saturation, warmth, and brightness. The safer development sequence is:

1. Build a standalone ColorSync probe that combines saturation, warmth, and
   brightness in one generated ICC profile.
2. Verify direction, reset behavior, smooth transitions, and multi-step preview.
3. Only after the probe is stable, replace the macOS split renderer with one
   unified ColorSync renderer.

A full ICC 3D LUT is also possible, but it is a larger color-management project:
it requires generating valid LUT tags, handling profile connection space details,
and validating behavior across different display profiles. It should not be the
first implementation step unless matrix composition proves insufficient.
