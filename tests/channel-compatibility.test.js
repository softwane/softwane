import test from "node:test";
import assert from "node:assert/strict";

import {
  getChannelConflictTooltip,
  isChannelBlockedByCompatibility,
} from "../src/channelCompatibility.js";

const conflicts = {
  saturation: ["color_temp", "brightness"],
  color_temp: ["saturation"],
  brightness: ["saturation"],
};

test("blocks warmth when saturation is already enabled on macOS", () => {
  const enabled = {
    saturation: true,
    color_temp: false,
    brightness: false,
  };

  assert.equal(
    isChannelBlockedByCompatibility("color_temp", enabled, conflicts, true),
    true,
  );
  assert.equal(
    getChannelConflictTooltip("color_temp", enabled, conflicts, true),
    "saturation",
  );
});

test("does not block the already enabled channel itself", () => {
  const enabled = {
    saturation: true,
    color_temp: false,
    brightness: false,
  };

  assert.equal(
    isChannelBlockedByCompatibility("saturation", enabled, conflicts, true),
    false,
  );
});

test("does not block incompatible channels outside macOS", () => {
  const enabled = {
    saturation: true,
    color_temp: false,
    brightness: false,
  };

  assert.equal(
    isChannelBlockedByCompatibility("brightness", enabled, conflicts, false),
    false,
  );
  assert.equal(
    getChannelConflictTooltip("brightness", enabled, conflicts, false),
    "",
  );
});
