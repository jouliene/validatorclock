// Which validators the server could not find a node for comes from the clock snapshot,
// not from the map: a chain the server resolves but has no map file for still knows, and
// the badge was being withheld because the page had nothing to draw the dots on.
function isFakeMapValidator(validator, options = {}) {
  if (!(options.fakeValidatorPeers instanceof Set)) {
    return false;
  }

  const publicKey = String(validator?.public_key || "").toLowerCase();
  return Boolean(publicKey) && options.fakeValidatorPeers.has(publicKey);
}

function fakeValidatorTypeTooltipLines(typeLabel, tooltipLines) {
  return Array.isArray(tooltipLines) && tooltipLines.length > 0
    ? tooltipLines
    : validatorTypeTooltipLines(typeLabel);
}

function validatorTypeTooltipLines(label, contractHash = "") {
  const entry = validatorTypeGlossaryEntry(label);
  const hash = String(contractHash || "").trim();
  const normalizedLabel = String(label || "").trim();
  const name = entry?.name || (normalizedLabel && normalizedLabel !== UNKNOWN_VALIDATOR_TYPE.label
    ? normalizedLabel
    : "Unknown");
  return [
    `Contract HASH: ${hash || "Unknown"}`,
    `Name: ${name}`,
  ];
}
