export const CHANNEL_CONFLICTS = {
  saturation: ["color_temp", "brightness"],
  color_temp: ["saturation"],
  brightness: ["saturation"],
};

export function getBlockingChannel(type, enabledChannels, conflicts = CHANNEL_CONFLICTS, isMacOS = false) {
  if (!isMacOS || enabledChannels[type]) return "";
  const blockedBy = conflicts[type] ?? [];
  return blockedBy.find((channelType) => enabledChannels[channelType]) ?? "";
}

export function isChannelBlockedByCompatibility(type, enabledChannels, conflicts = CHANNEL_CONFLICTS, isMacOS = false) {
  return Boolean(getBlockingChannel(type, enabledChannels, conflicts, isMacOS));
}

export function getChannelConflictTooltip(type, enabledChannels, conflicts = CHANNEL_CONFLICTS, isMacOS = false) {
  return getBlockingChannel(type, enabledChannels, conflicts, isMacOS);
}
